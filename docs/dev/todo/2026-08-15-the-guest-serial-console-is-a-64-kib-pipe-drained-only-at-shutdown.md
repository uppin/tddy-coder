# 2026-08-15 — The guest serial console is a 64 KiB pipe, drained only at shutdown

**Category:** Known failing test
**Source:** ci-vm-tests, 2026-08-15

- `QemuVmArgs::build_with_serial(config, "stdio")` (`qemu.rs:433`) gives the console-driven boot a
  **pipe**, whose capacity is the kernel's, not ours — 64 KiB by default, 1 MiB ceiling per
  `/proc/sys/fs/pipe-max-size`. A Debian boot with cloud-init writes more than that (measured:
  71,443 bytes), so a guest whose console nobody reads **blocks writing to `ttyS0`**.
- That is what broke shutdown in `accepts_ssh_as_the_policy_user_...`: `wait_for_ssh_ready` polls
  SSH and never pumps the console, so `systemd-shutdown` blocked and the guest never powered off.
  Fixed by draining concurrently with the port-release wait (`SerialConsole::drain_for`).
- **The fix is narrow on purpose.** The console is still undrained between boot and shutdown, so a
  guest can sit blocked on `ttyS0` for the whole body of a test. Nothing in the current suite needs
  the guest to make progress during that window, so all six pass — but a future test that does will
  hang the same way, and the symptom (an accepted powerdown that never completes, or a guest that
  mysteriously stalls) points nowhere near the cause.
- Options, if it ever needs to be properly unbounded:
  - Drain continuously in the background rather than only at shutdown. Keeps the console
    bidirectional, which the login-over-serial tests require.
  - `fcntl(F_SETPIPE_SZ)` to 1 MiB. One line, but it moves the cliff rather than removing it.
  - `-serial file:` is genuinely unbounded and never blocks — `QemuVmArgs::build` already uses it
    for detached boots — but it is **write-only**, so it cannot serve the tests that log in over the
    serial console.
- Related: this fixture keeps only an in-memory tail of the console, which is why diagnosing the
  above needed guests booted by hand to see what they were saying. QEMU's `-chardev …,logfile=PATH`
  would persist a full transcript alongside the interactive backend, making a CI failure readable
  from the uploaded artifact. The bake path already writes `<name>-boot.log`; this one does not.
