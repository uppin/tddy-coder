# Changeset: claude-sandbox-launcher — `./claude-sandbox` + `tddy-sandbox-app` YAML config + claude arg pass-through

**Date:** 2026-07-03
**Branch:** `master`
**Packages:** `tddy-sandbox-app`, `tddy-sandbox-runner`

## Summary

A one-command launcher for a sandboxed Claude Code session against the current directory as a
**managed** (unmounted) repo, with specialized subagents (e.g. an Ollama-hosted FastContext) wired
in from a single YAML config.

```bash
cd ~/my/project
claude-sandbox -c ~/sandbox-config.yaml -- "implement the login form"
```

## Changes

- **`./claude-sandbox`** (repo root, executable): symlink-safe root resolution so it runs from any
  CWD; passes `$PWD` as `--repo`; resolves host `claude` to an absolute path (jail PATH is only
  `/usr/bin:/bin`); builds `tddy-sandbox-app` + `tddy-tools` + `tddy-sandbox-runner` via
  `nix develop "$ROOT" --profile "$ROOT/.nix-profile"` into one target dir (they must sit as
  siblings), then execs the built binary on the host so it inherits `claude` on PATH. Flags:
  `-c/--config`, `--release`, `--no-build`; everything after `--` forwards to the in-jail `claude`.
- **`sandbox-config.example.yaml`** (repo root): starter config; `codebase_mode: managed` + an
  inline `fastcontext` subagent re-pointed at Ollama (`base_url: http://localhost:11434`).
- **`tddy-sandbox-app`**: new `--config <yaml>` (`config::SandboxAppConfig`, `deny_unknown_fields`).
  CLI flags override config. `subagents:` carries full inline `SpecializedAgentDef`s — declaring one
  both defines and activates it and overrides a same-named builtin, so `fastcontext` can be
  re-pointed at Ollama with no agents dir. New `config::resolve_session_agents` merges named +
  inline + `agents_dir` defs. `--model` is now optional (defaults after config merge). Trailing
  `-- <args>` (`#[arg(last = true)]`) forward to the in-jail `claude`.
  - `SpawnParams` now carries resolved `specialized_defs` + `claude_args` (replaces the old
    `SubagentSpawnConfig`; resolution moved to `config.rs`).
- **`tddy-sandbox-runner`**: new repeated `--claude-arg` (`allow_hyphen_values`), appended verbatim
  to the in-jail `claude` argv after the fixed flags + MCP allowlist (so a trailing positional
  prompt lands last). Ignored in `--pty-command` mode.

## Follow-up fixes (from live testing)

- **claude binary resolution** (`./claude-sandbox`): `command -v claude` resolved a **Superset
  wrapper shim** (`~/.superset/bin/claude`) that re-execs `claude` from PATH — which fails inside
  the jail (PATH is only `/usr/bin:/bin`). `resolve_claude()` now prefers `~/.local/bin/claude` and
  skips `*/.superset*/bin` on PATH; added a `--claude-binary` override.
- **claude arg ordering** (`tddy-sandbox-runner`): pass-through args were appended *after* the MCP
  block, whose trailing `--mcp-config` is variadic and swallowed a positional prompt (404 "MCP
  config file not found"). Args now go after the fixed flags and BEFORE the MCP args.
- **egress shim can't proxy plain HTTP → subagent 404** (`tddy-sandbox-runner`): the shim was
  CONNECT/HTTPS-only, so the subagent's plain-HTTP `POST http://localhost:11434/...` (absolute-form
  via `HTTP_PROXY`) hit the "everything else → 404" branch. Added a **forward-proxy path**:
  `rewrite_http_proxy_request` rewrites absolute-form → origin-form and extracts host:port;
  `handle_http_forward` opens a relay tunnel (host owns the outbound socket — no jail net rule
  needed) and streams. Refactored the CONNECT handler to share `open_relay_tunnel` + `pump_tunnel`.
  `base_url: http://localhost:11434` now works as-is.
- **persisted MCP/subagent logs + config knob** (logging): `write_claude_mcp_config` now writes an
  `env` block for the `tddy-tools --mcp` server; the runner sets `TDDY_TOOLS_LOG_FILE` →
  `<session-dir>/egress/tddy-tools.mcp.log` and `RUST_LOG` (default
  `info,tddy_tools=debug,tddy_discovery=debug`, override via `mcp_log_level` config / `--mcp-log-level`
  CLI / runner `--mcp-log-level`). `tddy-tools` `init_logging()` honors `TDDY_TOOLS_LOG_FILE`
  (append; falls back to stderr). App also maintains a `<session-base>/sessions/latest` symlink.

## TODO

- [x] Runner `--claude-arg` pass-through
- [x] `tddy-sandbox-app` `--config` + inline subagent defs + `-- <claude args>`
- [x] `./claude-sandbox` launcher + example config
- [x] claude-binary resolution past wrapper shims + arg ordering fix
- [x] egress shim plain-HTTP forward proxy (local model server reachability)
- [x] persisted in-jail MCP/subagent logs + `mcp_log_level` knob + `latest` symlink
- [x] Per-turn subagent logging (`tddy-discovery`) — request/response/error visible in the MCP log
- [x] 32K context via Modelfile variant (`fastcontext-tools-32k.Modelfile`)
- [x] Replaced tools hard-disabled via `--disallowedTools` (native + MCP form); `SemanticSearch`
      added to fastcontext `replaces`
- [ ] Feature doc under `docs/ft/` (managed-codebase + launcher usage) — follow-up
- [ ] Integration/acceptance test exercising a full sandboxed launch with an inline Ollama def

## Round 3 — replaced tools completely off

- **`build_claude_disallowlist` + `--disallowedTools`** (`tddy-sandbox-recipes/src/claude_cli.rs`):
  dropping a replaced tool from `--allowedTools` only un-pre-approves it — Claude's native built-in
  (`Grep`/`Glob`) and the still-advertised `mcp__tddy-tools__*` form remained reachable via the
  permission prompt. `append_claude_mcp_args` now also emits `--disallowedTools <native>` +
  `--disallowedTools mcp__tddy-tools__<tool>` for each replaced tool, so they're unreachable. Config
  `replaces` now includes `SemanticSearch` (delegated to fastcontext / disabled for the main agent).
- **Server-side enforcement (defense-in-depth)** — `tddy-tools` `PermissionServer::new()`
  (`server.rs`) now filters the advertised exec catalog by the replaced set
  (`resolve_replaced_tools_for_defs(&subagents_from_env())`) before merging it into the tool router,
  so a replaced tool is not advertised and cannot be invoked at the server — independent of Claude's
  allow/disallow lists. The subagent's own READ/GLOB/GREP loop is a separate in-process path
  (unaffected), so delegation still works.

## Follow-up fixes (round 2 — observability + model context)

- **Subagent HTTP loop was silent** (`tddy-discovery/src/subagent.rs`): the shared
  `send_turn_and_check_final_answer` now logs each turn (target `tddy_discovery::subagent`): request
  (model, message/tool counts), completion (elapsed, `finish_reason`, content length, tool-call
  count), and errors. Combined with the runner's `TDDY_TOOLS_LOG_FILE` wiring, fastcontext's
  behavior now lands in `<session>/egress/tddy-tools.mcp.log` instead of being invisible.
- **fastcontext ran away → "hang"**: ollama loaded `fastcontext-tools:latest` at its **4096**
  default; a single completion decoded ~24k tokens over 12m45s (repeated context-shift) → ollama
  500. Root cause: ollama's `/v1/chat/completions` **cannot set `num_ctx` per request** (verified:
  fresh-load test stayed 4096; upstream rejected it in ollama/ollama#6137). Fix (ollama-recommended
  Modelfile route): **`fastcontext-tools-32k.Modelfile`** (`FROM fastcontext-tools:latest` +
  `PARAMETER num_ctx 32768`) → `ollama create fastcontext-tools-32k`; verified it loads at
  **CONTEXT 32768 over /v1**. Configs point `model:` at the variant. No code change — the sandbox
  config's existing `model:` field is the knob.

## Unit tests

- [x] `packages/tddy-sandbox-app/src/config.rs` — config parse (ollama fastcontext), unknown-key
  rejection, empty-default, inline-def activate/override, named-builtin resolve, unknown-name error
- [x] Existing `spawn.rs` tests updated for the `specialized_defs`/`claude_args` `SpawnParams` shape

## Notes / follow-ups

- In managed mode the subagent's HTTP to `localhost:11434` is relayed to the host by the egress
  shim (same mechanism the default FastContext `:30000` already relies on).
- Full-launch smoke test verified: config loads → `codebase_mode=managed` → inline `fastcontext`
  activated. The interactive terminal-attach path was not exercised in CI.
