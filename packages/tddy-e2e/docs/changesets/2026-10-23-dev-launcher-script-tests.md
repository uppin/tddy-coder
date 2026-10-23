# 2026-10-23 — Behavioural tests for `./web-dev`'s LiveKit and process handling

**Type:** Feature

Cross-package entry: [2026-10-23-web-dev-livekit-and-visible-failures.md](../../../../docs/dev/changesets/2026-10-23-web-dev-livekit-and-visible-failures.md).

- **`tests/dev_livekit_script.rs`** runs `scripts/dev-livekit.sh` with stub `docker` and `curl`
  first on `PATH`, so no real Docker or network is needed. It covers:
  - the container is started on ephemeral, 1:1-published ports and avoids the daemon and Vite ports;
  - it is removed on exit, including error exits;
  - a taken port block is redrawn without touching its holder;
  - an external server is reused only with complete credentials, and an unreachable one is an error;
  - partial credentials name exactly the variables that are missing;
  - Docker not running, and a container that never becomes ready.
- **`tests/dev_process_script.rs`** runs `scripts/dev-process.sh` under the system `bash`, which is
  3.2 on macOS. It covers:
  - `dev_require_free_port`;
  - `dev_wait_any`;
  - port resolution: stable per checkout, pinning, and the conflict between `VITE_PORT` and
    `VITE_URL`;
  - `dev_wait_http_ready`;
  - the run record: reaping a dead run, skipping a reused pid, refusing while the owning run is
    alive, KILL escalation, and removing only `tddy-web-dev-livekit-*` containers.

  Every spawned process is held by a `Leftover` guard, so a failing test leaves nothing behind.
- **Static `web-dev` contract checks** go through `dev_script_contract::shell_code_lines`, which
  strips comments, so a comment cannot satisfy a check. Needles are anchored on whole command lines.
