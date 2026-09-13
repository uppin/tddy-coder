# 2026-08-15 — `echoes_a_message_over_sandbox_service_served_over_stdio` is skipped in CI

**Category:** Known failing test
**Source:** ci-setup, 2026-08-15

- `packages/tddy-daemon-sandbox/tests/sandbox_runner_stdio_acceptance.rs` — fails on a GitHub Actions runner
  with `tool ipc server exited before bind`, **with `tddy-sandbox-runner` built and on disk**. It
  survived two nextest retries, so it is a permission failure rather than a flake. The other two
  tests in the same binary pass once the runner binary is staged, so only this one is skipped.
- Same family as the sandbox set above: an unprivileged process cannot place its own child in a
  limited cgroup scope. The distinguishing detail is that the failure surfaces here as a *silent
  runner exit before bind* rather than a named `EPERM`, so the runner is swallowing the real error —
  whoever picks this up should make it report the syscall that actually failed before theorising.
- Skipped via `default-filter` in `.config/nextest.toml` (`[profile.ci]`), which names this file.
- **Long-term fix: run these under the VM testkit rather than on the runner.** A QEMU guest gives a
  fully controlled environment with real root and a writable cgroup root, which is the only way this
  suite and the rest of the sandbox set become genuine CI coverage instead of permanent exclusions.
  See `docs/ft/vm/tddy-vm.md` § VM testkit and the `./vm-tests` script; the open question is cost,
  since the bakes currently take hours and `TDDY_CLOUDINIT_BASE_IMAGE` is never downloaded.
