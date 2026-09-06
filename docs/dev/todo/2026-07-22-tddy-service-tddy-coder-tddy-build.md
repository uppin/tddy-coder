# 2026-07-22 — tddy-service / tddy-coder / tddy-build

**Category:** Future enhancement
**Source:** bsp-build-server changeset, 2026-07-22

- **Literal JSON-RPC 2.0 BSP transport** — the `bsp.BspService` is BSP-*shaped* over the workspace's
  protobuf/Connect + LiveKit RPC, so external BSP clients (Metals, IntelliJ-BSP) cannot connect. A real
  JSON-RPC 2.0 BSP server (`build/initialize`, `workspace/buildTargets`, …) over stdio/TCP with a `.bsp/`
  connection file would require a new transport.
- **Structured build diagnostics** — build ops return exit code + raw stdout/stderr only. Parse compiler
  output into `{file, line, severity, message}` diagnostics for `BuildTargetCompile`/`Test` responses.
- **Remaining BSP methods** — `buildTarget/inverseSources`, `dependencySources`, `dependencyModules`,
  `resources`, `debugSession/start`.
- **Unify shared `catalog` build_target rows with `build_targets`** — the populate task currently writes
  build targets into both the shared `catalog` table (lightweight, for the unified `list`) and the new
  `build_targets` table (rich, for BSP). Collapse the duplication via a SQL view once the read paths agree.
- **Streaming compile/test progress** — the initial compile/test/run methods are unary; add BSP-style task
  progress notifications (server-streaming) for long builds.
- **BSP per-request session construction (no cache)** — the daemon's `DaemonBspService` builds a fresh
  `BspServiceImpl` per request (each triggers a catalog open+populate). Fine for correctness; add a
  per-session cache (keyed by resolved `session_dir`, with eviction) if the read path gets hot.
- **Silent provider lowering failures** — `tddy-bsp`'s provider swallows a target's lowering error and lists
  it with empty sources/outputs (no `log` dependency on `tddy-bsp`). Add `log` and a `debug!` on failure if
  that observability is wanted.
