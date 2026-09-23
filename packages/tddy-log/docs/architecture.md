# tddy-log architecture

## Overview

The tddy log sink — config-driven loggers and policies, multi-output routing and startup rotation
— and the guards that keep log output off a stdio that a TUI or an RPC relay owns. A leaf: no
workspace dependencies.

`tddy-core` re-exports this crate whole (`pub use tddy_log::*;`), so every `tddy_core::{log_backend, stdio_safety}::…` path consumers name resolves unchanged. New code should name `tddy_log` directly.

## Log (`log_backend.rs`)

- **LogConfig**: YAML `log:` section. **Loggers** define output targets (stderr, stdout, file, buffer, mute) and optional format. **Policies** reference loggers by name and map selectors (target, module_path, heuristic) to level filters. First-match-wins ordering.
- **TddyLogger**: Implements `log::Log`. Routes records to the logger chosen by the first matching policy. Format templating: `{timestamp}`, `{level}`, `{target}`, `{module}`, `{message}`.
- **Log rotation**: On startup, existing file outputs are renamed to `{stem}.{ISO-8601}.{ext}`; rotated files beyond `max_rotated` are pruned. `TDDY_QUIET` switches default output to buffer for TUI display.


## Stdio safety (`stdio_safety.rs`)

Guarantees fd 1 (stdout) carries only RPC frames when a process runs with `--stdio` (RPC over stdin/stdout via `tddy-stdio`), which has zero tolerance for stray bytes.

- **enforce_stdio_safe_log_output**: Force-rewrites any `LogOutput::Stdout` logger destination in a `LogConfig` to `LogOutput::Stderr` before `init_tddy_logger` runs (which can only be called once). Leaves `Stderr`/`File`/`Buffer`/`Mute` untouched. Returns the number of loggers changed.
- **redirect_fd_to_file** (`#[cfg(unix)]`): Redirects an arbitrary file descriptor to a log file via `dup2`, creating the file first so the target fd is untouched on failure. Generalizes the `--daemon` stderr-redirect pattern (`tddy-coder`'s `run.rs`) for `--stdio`'s use case: unlike `--daemon` (stdin/stdout/stderr all null), `--stdio` must keep stdin/stdout live for RPC framing — only stderr moves to a file.
- Consumers: `tddy-coder`'s `--stdio` dispatch and `tddy-sandbox-runner`'s `--stdio` dispatch both call these before serving any RPC traffic. See [rpc-multi-transport.md](../../../docs/ft/coder/rpc-multi-transport.md) and [grpc-remote-control.md](../../../docs/ft/coder/grpc-remote-control.md) for the transports this protects.
