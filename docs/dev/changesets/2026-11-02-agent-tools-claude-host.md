# 2026-11-02 — Claude Code CLI launches from its tool definition

**Type:** Feature · **Stack:** `#agent-tools` 2/12 ([#791](https://github.com/uppin/tddy-coder/pull/791)), base `feature/agent-tools/tool-defs` ([#790](https://github.com/uppin/tddy-coder/pull/790)) · **Packages:** tddy-actions, tddy-coding-agent-specs (new), tddy-agent-launch, tddy-cli-sessions, tddy-session-split, tddy-session-lifecycle, tddy-daemon

## State A

- A `claude-cli` launch was assembled by hand across several places:
  - the argv in `claude_argv.rs` / `cli-sessions/argv.rs`;
  - the binary in `agent_cli.rs`;
  - the hooks file in `claude_hooks.rs` / `install_claude_cli_hooks`;
  - a project-home main's allow/deny/MCP flags pre-rendered in `project_home_launch`.
- `ToolDef` had no `run:` and no extension mechanism.

## State B

**The tool schema.** `ToolDef` gains:

- `run:`, an argv template rendered by `render_argv`. Its items are literals with `{name}`, `When` and `Splice`, and an unbound name is an error.
- open, owner-validated `extensions:` (`ExtensionOwner`, `load_with_owners`).

`tddy-tools.yaml` is a built-in support tool.

**The new crate `tddy-coding-agent-specs`** owns the `coding_agent` extension and `claude.yaml`, plus the slot renderers:

- the MCP file with `strict_flag`;
- the allow/deny/permission-prompt flags (`ToolSlot = Flag | unsupported`);
- the instruction files;
- the `claude-settings-local` hooks adapter.

**The launch.** The daemon installs `host_coding_agents(<tddyhome>/tools)` on the session host (`with_coding_agents`). Unsandboxed `claude-cli` start and resume then go through `launch_claude_cli` → `launch_host_pty`, and the claude spec's slots provide:

- the hooks file;
- the appended system prompt;
- a main's `--allowedTools` / `--disallowedTools` and its `tddy-tools` MCP registration.

An ordinary session's argv, launch environment and hooks file are byte-identical to State A. The goldens are captured from the hand-built launch. A host without coding agents keeps the hand-built launch.

**Not slotted**, and passed as rendered flags after the slot flags:

- a main's role prompt, because the slot takes one prompt file and a managed recipe's prompt can hold it;
- a split session's wiring, which belongs to `#agent-tools` 10/12.

Product: [claude-cli-session.md § Launch](../../ft/daemon/claude-cli-session.md#launch-from-the-claude-tool-definition), [sandbox-builder.md § Declared tools](../../ft/coder/sandbox-builder.md#declared-tools). Package docs:

- [coding-agent-specs.md](../../../packages/tddy-coding-agent-specs/docs/coding-agent-specs.md)
- [declared-tools.md](../../../packages/tddy-actions/docs/declared-tools.md)
- [tddy-agent-launch module-layout.md](../../../packages/tddy-agent-launch/docs/module-layout.md)

## Deviations from the planned surface

- **`ArgvItem::Splice`.** The slot flags sit between the permission flags and the prompt.
- **The `claude.yaml` argv is byte-identical to the hand-built one:**
  - its permission arm is `dangerously_skip_permissions` / `--permission-mode` (default `auto`);
  - `--model` sits under `if: model`;
  - it has `env: {}`, because `CLAUDE_CODE_NO_FLICKER` stays in the launcher (daemon.yaml `claude_cli.fullscreen`).
- **`BUILTIN_SUPPORT_TOOLS`** is separate from `BUILTIN_TOOLS`, because #790 pins the jail tool set.
- **`McpSlot::File.strict_flag`** is added. Without it, a variadic `--mcp-config` placed before the prompt would swallow the prompt.
- **`host_coding_agents`** lives in `tddy-agent-launch`, not `tddy-daemon`. `unbundle_endpoint` admits only wiring modules in the daemon.

## Decisions for the developer

- **A bad `<tddyhome>/tools/*.yaml` fails daemon startup**, naming the file. That covers an unknown field, an unknown slot kind and an unclaimed extension key. The alternative is to skip the bad file with a warning, which is a fallback. It was not taken, because CLAUDE.md requires consent for fallbacks.
- **On the spec launch, a hooks file that cannot be written fails the start.** The hand-built launch warns and continues. Fail-loud was kept, with no fallback.

## Backlog and code issues

- **Backlog entry added:** `docs/dev/todo/2026-11-02-claude-cli-hand-built-launch-removal.md`. The hand-built launch goes with #801 (`TODO(agent-tools 12/12)` on `HostCodingAgents`).
- **Backlog entry left in place:** `2026-10-24-claude-agent-tab-shares-session-hooks` stays (⚠ During). The hooks slot takes its params per launch, so a per-tab identity becomes a parameter later.
- `complexity-svc-resume-claude-cli-session-resume-claude-cli-session`: 141 → 139 lines. Narrowed, still open.
- `complexity-claude-cli-spawn-spawn-claude-cli-session-inner`: **added** at 318 lines (309 at base).
- `oversized-file-svc-host-builders`: 503 → 504. The file is stack-shared, so its split is deferred.
- `complexity-svc-start-session-core-start-session-core`: not touched.

## Validation (2026-11-02, `/validate-changes` + `/pr-wrap`, macOS)

- **Stack gate:**
  - rebased with `--onto origin/feature/agent-tools/tool-defs` (`6bbd35ee4`) from the old base tip `37912f334`;
  - one conflict in `tool/error.rs`, resolved by union (#790's `DuplicateCommand` plus this PR's three variants);
  - leak check clean.
- **Gaps sent back to green once and closed:**
  - the hooks file moved behind the hooks slot (the `TODO(agent-tools 2/12)` is gone);
  - a main's allow/deny/MCP flags render through the slots;
  - `coding_agents.rs` moved out of `tddy-daemon`, which fixed `unbundle_endpoint::every_module_left_in_the_daemon_is_one_of_the_endpoint_set`.
- **Cleanups:**
  - `spawn_claude_cli_process` is split per launch path (74 → 21 lines);
  - `launch_claude_cli` went from 61 to 49 lines;
  - the test-only `project_home_launch` helper left production code.
- **Scoped gates:**
  - `./test -p tddy-actions -p tddy-coding-agent-specs -p tddy-agent-launch -p tddy-cli-sessions -p tddy-session-split -p tddy-session-lifecycle -p tddy-daemon`: 1392 passed, 24 failed, 2 ignored. All 24 failures are environmental on this Mac:
    - 16 × `sandbox RPC bridge not installed`;
    - 6 × `session_sync_livekit_acceptance` (`tddy-remote-git-repo` not built);
    - `session_agent_remote_acceptance::fails_only_the_agents_of_a_daemon_that_goes_away`. The suite is marked `FIXME(flaky)`; it passes alone;
    - `claude_cli_from_spec_acceptance::an_unsandboxed_…`, which needs a canonical TMPDIR on macOS. The nix shell's `/tmp/nix-shell.*` resolves through the `/tmp` → `/private/tmp` symlink, while CI runs Linux, where `/tmp` is canonical. The suite passes 5/5 with a canonical TMPDIR.
  - `cargo clippy -p <the seven> --all-targets -- -D warnings`: clean.
  - `cargo fmt`: clean.
