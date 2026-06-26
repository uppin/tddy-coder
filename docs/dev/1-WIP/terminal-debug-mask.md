# Changeset: terminal-debug-mask — browser DEBUG mask from daemon config

**Date:** 2026-06-26  
**Branch:** `terminal-debug-mask`  
**Packages:** `tddy-daemon`, `tddy-coder`, `tddy-web`  
**Feature doc:** [docs/ft/web/local-web-dev.md § Browser DEBUG mask](../../ft/web/local-web-dev.md#browser-debug-mask)

## Goal

Add a config-driven browser DEBUG mask so `./web-dev` can switch on scoped `[tddy]` console diagnostics —
primarily to debug terminal **garbling / misalignment**. The daemon exposes a `debug`-package namespace
mask at `GET /api/config`; the browser adopts it, with a per-browser `localStorage` override that survives
across sessions and is invalidated only when the config value changes.

## Delta summary

### `tddy-daemon`

- `config.rs` — `DaemonConfig.debug: Option<String>` (`#[serde(default)]`); tests for default/parse.
- `server.rs` — `run_server` gains a `debug: Option<String>` param, set on `ClientConfig.debug`.
- `main.rs` — reads `config.debug` → `web_debug` → `run_server`.
- `dev.daemon.yaml` — ships `debug: "tddy:term:*"` with documentation of namespaces/precedence.

### `tddy-coder`

- `web_server.rs` — `ClientConfig.debug: Option<String>` (`skip_serializing_if = "Option::is_none"`),
  served at `/api/config`; `#[cfg(test)] mod tests` asserts the field is omitted when `None` and emitted
  when `Some`.
- `run.rs` — standalone `build_client_config` sets `debug: None` (no daemon mask in CLI mode).

### `tddy-web`

**New files:**
- `src/lib/debugMask.ts` — `resolveDebugMask` (pure precedence/invalidation), `applyDebugMaskFromConfig`,
  `applyDebugMaskFromUrl` (`?debug=`), `tddyDebug(namespace)` factory, `enableDebugMask`. Built on the
  `debug` package (promoted to a direct dependency; already resolved transitively in `bun.lock`).
- `src/debug.d.ts` — minimal ambient types for `debug` (avoids pulling `@types/debug`).
- `src/lib/debugMask.test.ts` — 6 bun unit tests covering adopt / keep-override / invalidate / clear /
  no-op / trim.

**Modified files:**
- `index.tsx` — applies `?debug=` at boot; both `/api/config` fetch handlers call
  `applyDebugMaskFromConfig(config?.debug)`.
- `components/GhosttyTerminal.tsx` — replaced the ad-hoc `console.log` spam + `debugLogging` `log()` helper
  with namespaced loggers (`tddy:term:{life,data,write,resize,mouse}`); `debugLogging` prop still
  force-enables. `write` logs byte length + hex/escaped preview.
- `components/GhosttyTerminalGrpc.tsx` — `tddy:term:grpc` logging on the byte stream: recv length,
  ready/buffered state, hex preview, buffered-flush count, resize sequence.
- `package.json` — `debug: ^4.4.3` direct dependency.

## Precedence

`?debug=` URL param (session) → `localStorage.debug` (persistent per-browser override) → `debug:` from
`/api/config` (baseline; re-applied/invalidating the override only when it changes) → off.

## Unit tests

- [x] `packages/tddy-web/src/lib/debugMask.test.ts` (6 pass)
- [x] `tddy-daemon` `web_debug_mask_tests` (default / absent-in-yaml / parse)
- [x] `tddy-coder` `web_server::tests` (debug omitted when None / serialized when set)

## Notes

- `run_server` signature change rippled into `tests/relay_idle_shutdown_acceptance.rs` and
  `tests/relay_e2e_acceptance.rs` (added the `debug` arg).
