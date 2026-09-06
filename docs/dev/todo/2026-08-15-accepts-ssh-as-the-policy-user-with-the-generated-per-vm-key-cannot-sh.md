# 2026-08-15 — `accepts_ssh_as_the_policy_user_with_the_generated_per_vm_key` cannot shut its guest down in time

**Category:** Known failing test
**Source:** ci-vm-tests, 2026-08-15

- `packages/tddy-vm/tests/vm_boot_control_acceptance.rs:297` fails with `guest accepted the
  powerdown but never released port 2235`. Deterministic, not a flake: it fails the same way run
  alone. It is the **only** remaining failure in the boot-control suite on x86_64; the other five
  pass.
- What is *not* wrong, established by experiment rather than reading: ACPI powerdown works fine on
  this guest. A manually booted guest that finished cloud-init and never saw an SSH connection
  powered off in **~40 s** — `Reached target poweroff.target` → `reboot: Power down`. The
  `shuts_a_running_vm_down_gracefully_via_the_qemu_monitor` test, which never opens an SSH session,
  also passes.
- So the distinguishing factor is the SSH session this test opens. The likely mechanism is systemd
  waiting on the `user@<uid>.service` / session scope during shutdown, which on Debian is bounded by
  `DefaultTimeoutStopSec` (90 s). Roughly 40 s + 90 s exceeds `BootedGuest`'s `SHUTDOWN_TIMEOUT` of
  120 s (`packages/tddy-vm-testkit/src/guest.rs`), which fits the observation — but the actual
  duration was **not measured**, so confirm it before choosing a number.
- Two candidate fixes, and the choice is a real one: raise `SHUTDOWN_TIMEOUT` past the measured
  worst case, or make the session teardown deterministic (close the connection and wait for the
  session to end) so shutdown does not depend on a systemd timeout at all. The second is a readiness
  signal rather than a budget, and is preferable if the session can be observed ending.
