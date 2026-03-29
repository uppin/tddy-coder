# Clean Code Analysis — Remote Sandbox Feature

Cross-reference: [evaluation-report.md](./evaluation-report.md) (changed paths and validity).

## Summary

The remote sandbox work is **structurally clear**: proto defines the contract, `tddy-service` hosts `RemoteSandboxServiceImpl`, `tddy-remote` exposes CLI + Connect unary client + rsync bridge, and `tddy-daemon` exposes a small `remote_sandbox` module (VFS helpers). Naming is mostly consistent (`session`, `argv_json`, Connect paths).

The main quality gaps are **triplicated relative-path logic** (daemon VFS, service sanitizer, client normalizer), a few **naming mismatches** for the same operation, **stub/skeleton markers** that may be outdated, and **concentrated complexity** in `sync_byte_bridge` (poll-based I/O) without smaller extracted steps. Security and auth are called out in the evaluation report and are out of scope for “clean code” alone but belong in any hardening pass.

## Strengths

- **Clear module boundaries**: `packages/tddy-remote/src/connect_client.rs` isolates HTTP/Connect unary calls; `config.rs` handles YAML; `session.rs` and `rsh.rs` handle user-facing flows. `lib.rs` re-exports are minimal and readable.
- **Useful high-signal comments**: `packages/tddy-remote/src/rsh.rs` documents why poll + ordering matter for rsync; `packages/tddy-service/src/remote_sandbox_service.rs` explains `--mkpath` injection and the rsync TCP bridge intent.
- **Consistent RPC patterns**: JSON `argv_json` for argv arrays appears in proto and both client and server paths; session keys flow through `PutObject`, `OpenRsyncSession`, and `ExecNonInteractive` where applicable.
- **Small daemon surface**: `packages/tddy-daemon/src/remote_sandbox/vfs.rs` is focused and testable; `mod.rs` stays thin.

## Issues

### Duplication (relative path rules)

The same “safe relative path under sandbox root” logic appears in three places with small API differences:

| Location | Function | Return type |
|----------|----------|-------------|
| `packages/tddy-daemon/src/remote_sandbox/vfs.rs` | `ensure_relative_under_root` | `Result<(), &'static str>` |
| `packages/tddy-service/src/remote_sandbox_service.rs` | `sanitize_relative_path` | `Result<PathBuf, Status>` |
| `packages/tddy-remote/src/vfs_path.rs` | `normalize_sandbox_relative_path` | `Result<PathBuf, &'static str>` |

Drift risk is real: e.g. daemon `vfs.rs` treats empty normal segments differently (continues) vs service/client (push/accumulate). Unifying behavior in one place would reduce subtle divergence.

### Naming inconsistency

- Same concept: **ensure** vs **sanitize** vs **normalize** — pick one verb for the public API and mirror it in tests/docs.
- `io_err` helpers are duplicated in `packages/tddy-remote/src/rsh.rs` and `packages/tddy-remote/src/session.rs` (minor).

### Module docs vs reality

- `packages/tddy-daemon/src/remote_sandbox/mod.rs` says “skeleton for Red phase”; the feature is substantially implemented elsewhere. Either update the line or remove if misleading.
- `packages/tddy-remote/src/session.rs`: `run_shell_pty` takes `pty: bool` but the body uses a fixed non-interactive bash echo path; the parameter is logged only. That is honest in comments but **API noise** until PTY is real.

### Complexity / size

- `packages/tddy-remote/src/rsh.rs`: `sync_byte_bridge` (~lines 29–254) is a single large function handling poll setup, stdin/stdout/socket state, and shutdown rules. It is documented but hard to unit-test in isolation.
- `packages/tddy-service/src/remote_sandbox_service.rs`: `RemoteSandboxService` impl aggregates registry, exec, VFS, checksum smoke, and rsync bridge — acceptable for a service façade, but `exec_checksum` ignores the request type and hardcodes session `"default"` (intentional smoke, but stands out in a “clean” read).

### Config and dead fields

- `packages/tddy-remote/src/config.rs`: `default_authority` in `RemoteYaml` is parsed but unused (already noted in evaluation-report). Either wire it into `resolve_connect_base` when `host` is ambiguous or remove until needed.

### Inline documentation noise vs value

- Most module-level `//!` headers add value. Occasional `log::debug!` + repetitive `map_err` chains in `session.rs` / `rsh.rs` are fine but not “documentation” — no change required unless you want a tiny shared `connect_call` helper to shrink repetition.

## Recommendations (refactor ideas)

1. **Single source of truth for path rules**  
   Extract a small shared function or crate used by daemon tests, `tddy-service`, and `tddy-remote` (e.g. `fn normalize_relative_sandbox_path(raw: &str) -> Result<PathBuf, PathError>` with one error type mapped to `Status` / `&'static str` at edges). Align edge cases (empty segments, `..`, UTF-8) once.

2. **Rename for consistency**  
   After consolidation, use one name (e.g. `normalize_sandbox_relative_path` everywhere, or `sanitize_relative_path` everywhere) and keep daemon-specific names only if the behavior truly differs.

3. **Split `sync_byte_bridge`**  
   In `packages/tddy-remote/src/rsh.rs`, extract: (a) poll fd vector construction, (b) one iteration’s “drain stdin → socket”, (c) “socket → stdout”, (d) shutdown policy. Keeps behavior identical but improves testability and reviewability.

4. **Clarify `run_shell_pty`**  
   Until PTY is implemented: consider `#[allow(unused)]` on `pty` with a one-line FIXME, or a subcommand variant that does not advertise `--pty` — avoids implying behavior that is not there.

5. **Refresh `remote_sandbox/mod.rs` docs**  
   Replace “skeleton for Red phase” with a one-sentence description of what the daemon module provides today (VFS helpers for tests/rules).

6. **`default_authority`**  
   Implement resolution fallback or remove the field from the struct until the product needs it (reduces confusion and compiler/lint noise).

7. **`exec_checksum`**  
   If it remains a smoke-only RPC, document that explicitly next to the method (proto or impl) so readers do not assume it hashes arbitrary user paths from the request.

---

**Files reviewed (primary production code):**

- `packages/tddy-daemon/src/remote_sandbox/mod.rs`, `packages/tddy-daemon/src/remote_sandbox/vfs.rs`
- `packages/tddy-service/src/remote_sandbox_service.rs`, `packages/tddy-service/proto/remote_sandbox.proto`
- `packages/tddy-remote/src/lib.rs`, `main.rs`, `config.rs`, `connect_client.rs`, `session.rs`, `rsh.rs`, `vfs_path.rs`

Integration tests and stubs were not exhaustively reviewed for style; they follow the same patterns as the rest of the crate tests.
