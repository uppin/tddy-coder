# 2026-07-01 — stdio-safe core

**Type:** Feature

new `stdio_safety` module (`enforce_stdio_safe_log_output`, `redirect_fd_to_file`) guarantees fd 1 carries only RPC frames for `--stdio` binaries; generalizes `--daemon`'s stderr-redirect pattern. Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-core)
