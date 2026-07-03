# Guest image: 9p support + init hook

`tddy-sandbox-qemu` boots a Buildroot-produced image (via `tddy-vm-build`) and expects the
guest kernel/rootfs to already have virtio-9p support and a init script that wires the
reserved 9p shares to `tddy-sandbox-runner`. Buildroot images built without this fragment
+ overlay **cannot** be used as a `tddy-sandbox-qemu` `--image` — they have no way to reach
`plan.json` or the injected runner binary.

This is a documentation-only deliverable: `build_image`/`build_vm_image_from_spec`
(`packages/tddy-vm/src/build.rs`) take a caller-supplied Buildroot `.config` spec as an
opaque string and don't currently bundle extra files (kernel fragments, rootfs overlays)
into the pipeline themselves. Until that's added, a caller assembling a spec for a
`tddy-sandbox-qemu`-capable image must include the lines below and reference the files in
this directory by their absolute path on the build host.

## 1. Buildroot spec additions

Add to the `.config` spec passed to `tddy-vm-build`/`build_image`:

```
BR2_LINUX_KERNEL_CONFIG_FRAGMENT_FILES="/absolute/path/to/tddy-coder/packages/tddy-sandbox-qemu/guest/linux-9p.fragment"
BR2_ROOTFS_OVERLAY="/absolute/path/to/tddy-coder/packages/tddy-sandbox-qemu/guest/rootfs-overlay"
BR2_PACKAGE_JQ=y
```

- **`linux-9p.fragment`** ([source](../guest/linux-9p.fragment)) — kernel `CONFIG_NET_9P*`/
  `CONFIG_9P_FS*`/`CONFIG_VIRTIO*` symbols. This is a *kernel* Kconfig fragment (merged into
  the kernel's own `.config` via `genconfig.sh` during the kernel build step), not a
  Buildroot package option — 9p is virtio transport + filesystem driver support, which
  lives in the kernel, not in a Buildroot package.
- **`rootfs-overlay/`** ([source](../guest/rootfs-overlay)) — copied verbatim onto the
  target rootfs after Buildroot's own build, so `etc/init.d/S99tddy-sandbox` ends up at
  that exact path in the image.
- **`BR2_PACKAGE_JQ=y`** — the init hook needs a real JSON parser for `plan.json`'s nested
  `mounts` array and `env` object; BusyBox `awk`/`sed` alone can't do this robustly. `jq` is
  a small (~1 MB), well-tested Buildroot package — pulling in a scripting runtime (Python/
  Lua) just to parse one JSON file would be disproportionate.

Multiple space-separated paths are valid for `BR2_ROOTFS_OVERLAY`; if the image also needs
another overlay, append this one rather than replacing it.

## 2. What the init hook does (`S99tddy-sandbox`)

Runs as the last BusyBox `sysvinit` script (`/etc/init.d/S99tddy-sandbox`, invoked by
`rcS` in numeric order). At a high level:

1. Mounts the two reserved 9p shares read-only: `tddy-plan` (carries `plan.json`, written
   host-side by `plan_config_dir()`/`guest_plan_json()` in
   [`../src/argv.rs`](../src/argv.rs)) and `tddy-runner` (the directory containing the
   cross-compiled `tddy-sandbox-runner` binary, see
   [`../../tddy-sandbox-runner/docker/musl-cross/`](../../tddy-sandbox-runner/docker/musl-cross/)).
2. Reads `plan.json`'s `mounts` array and mounts each host-declared share (tags
   `tddy-mount0`, `tddy-mount1`, ... — assigned by *array position*, see the gotcha below)
   at its `jail` path (or `host` path, if `jail` is `null` — same "None = same path"
   convention as `MountSpec` itself), read-only unless `writable` is `true`.
3. Builds `tddy-sandbox-runner`'s full argv (`--session-id`, `--context-dir`, dummy
   `--grpc-socket`/`--tool-ipc-socket` paths under `/run` — required flags, but unused
   in TCP mode; see [`runner.rs`](../../tddy-sandbox-runner/src/runner.rs) — `--model none`,
   `--ready-marker`, `--grpc-listen-port <plan.json's control_port>`, and one
   `--pty-command=<token>` per `plan.json` `command` element) and execs it in the
   background, redirecting output to `/dev/console` (visible via the `-serial file:...` log
   `qemu_sandbox_argv` already sets up, for post-mortem debugging).

### Why S99 backgrounds the runner instead of a foreground exec

An earlier draft considered `exec`ing the runner directly as the last step of `rcS`,
replacing the init script's own process. That would block the rest of the boot sequence
(any `S*` scripts after `S99tddy-sandbox`, plus `rcS`'s own completion) on the runner's
entire lifetime — harmless in the common case, but it means a boot-diagnostics shell
(`getty` on a serial console, if the image has one) never becomes available if the runner
hangs during startup, which is exactly the scenario you'd want a shell to debug. Running
it as a background job (`&`) lets normal boot finish either way, at the cost of needing an
explicit `/dev/console` redirect to keep the runner's own output visible (backgrounded
processes lose their controlling terminal).

### Gotcha: mount tag assignment is positional, not named

`qemu_sandbox_argv` assigns 9p `mount_tag`s to plan-declared mounts by loop index
(`tddy-mount{index}` — see [`../src/argv.rs`](../src/argv.rs)), and `guest_plan_json`
writes the same `plan.mounts` slice, in the same order, into `plan.json`'s `mounts` array.
The init hook relies on this ordering coupling implicitly (`jq`'s `to_entries[].key` for
array index == the `-fsdev`/`-device` loop's `index`). If either side's mount ordering
ever changes independently (e.g. sorting, filtering, or reordering `plan.mounts` on just
one side), the two would silently desync — the init hook would mount the wrong host
directory at a given jail path. Anyone changing mount handling on either side should
re-verify this positional contract holds.

## 3. Not yet done

- **Not tested against a real boot.** The init hook's logic (mount-index extraction,
  command/env argv construction) was verified with `dash`/`jq` on the host, simulating
  `plan.json` inputs including a path containing spaces (an earlier draft that built
  `--pty-command` flags via string concatenation + word-splitting silently broke on
  exactly that case — fixed by having `jq` construct the entire runner argv as
  individually-`@sh`-quoted `set --` tokens instead). It has **not** been exercised inside
  an actual booted Buildroot+9p guest — that requires a real image built with the fragment
  above, which doesn't exist yet (no current Buildroot image in this repo has 9p enabled).
- No retry/timeout if a 9p mount fails (e.g. host share not ready yet) — `set -e` means the
  script aborts, leaving the guest without a functioning runner and no automatic recovery.
  Given QEMU's own boot ordering (the 9p `virtio-9p-pci` devices are present from qemu
  launch, not hot-plugged later), this is expected to be a non-issue in practice, but isn't
  proven by a real boot yet.
- `plan.spec.profile_path`/secrets/egress-shim wiring (used by the darwin/cgroups backends
  for MCP tool calls and network egress policy) has no guest-side equivalent here yet —
  `tddy-sandbox-qemu`'s host side already runs with `NullToolHandler` (see
  `run_sandbox_qemu` in [`../src/lib.rs`](../src/lib.rs)), so this only matters once
  tool-calling support is added to the QEMU backend.
