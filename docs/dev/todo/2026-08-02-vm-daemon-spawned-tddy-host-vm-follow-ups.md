# 2026-08-02 — VM — daemon-spawned tddy host VM follow-ups

**Category:** Future enhancement
**Source:** daemon-spawned-tddy-host-vm changeset, 2026-08-02

- **`SerialConsole` should be able to quiesce the guest kernel console.** `ttyAMA0` is shared
  between the login shell and the kernel log, so a `printk` landing mid-command is captured as
  another line of that command's output (observed: a `mount` returning
  `[ 7.790602] 9p: Installing v9fs 9p2000 file system support`). The acceptance tests now run
  `dmesg -n 1` after login to make exact-output assertions deterministic, but any production
  consumer driving a guest over UART faces the same interleaving — a `quiesce_kernel_console`
  on the driver is the natural home.
- ~~**A possible ordering bug in the cloud-init completion script (found statically,
  unverified).**~~ **Confirmed by a real bake and fixed (2026-08-14).** `scripts_per_boot` does
  run before `scripts_user`, so the completion script halted the guest before `runcmd` ever ran
  and the host sealed a half-baked image as a success; `cloud-init status --wait` did not block
  because the seed's `cloud-init clean --logs --seed` `bootcmd` had wiped the status it waits on.
  The completion signal now lives in `runcmd` itself — a preamble step arming an EXIT trap that
  emits `<token>_FAILED`, and a final step that emits the success token — and both paths dump
  the guest's `/var/log/cloud-init.log` and `/var/log/cloud-init-output.log` to the console,
  framed by `TDDY_GUEST_LOG_BEGIN`/`TDDY_GUEST_LOG_END`, into a boot log now written with its
  terminal escapes stripped.
- **Guest console log-level control.** The bake streams every serial line as RPC progress —
  ~713 lines in the first 17 seconds, almost all kernel and systemd chatter. `cloud_init_boot_argv`
  and `QemuVmArgs::build` emit no kernel cmdline at all, so there is no `loglevel=`/`quiet` knob.
  There is no prior art to copy: `~/Code/makers-lt` has no guest-side loglevel control either
  (no `printk`, `dmesg -n`, or `console=`, and its `qemu-vm-builder` exposes no `-append`); it
  handles noise host-side via sentinel matching and a `debug`-namespace gate. Deferred until the
  bake's real signal-to-noise is known.
- **`VmManager::start` still rejects `build_target` specs** (`registry.rs:177-180`) — only
  `image_path` works. Untouched by this changeset.
- **The bake pays a kernel swap it may not need.** Every `genericcloud` base costs an extra
  ~3 min, a ~100 MB download and a reboot to get a 9p-capable kernel. Supplying a Debian
  *generic* base image skips it entirely (the step is guarded on `uname -r`), so documenting
  or defaulting to a generic base would remove the cost. Alternatively, shipping the working
  copy as a second ISO9660 disk needs no 9p at all — iso9660 and virtio-blk are in the cloud
  kernel, as the seed ISO already proves.
- **`QemuVm` reports no real guest exit code from `deploy`.** Now that `serial_shell` can capture
  an exit code over UART, the SSH path's error reporting could be brought up to the same standard.
- **Reuse `serial_shell` for the cloud-init bake's completion detection.** `build_cloud_init_image`
  still uses the single-token `classify_serial_line` matcher; with a console driver available it
  could log into a guest that failed and interrogate it, instead of only reporting that the token
  never arrived.
- **A `ListPreparedBases` RPC.** `CreateVmFromPreparedBase` takes a prepared-base name with no way
  to discover what has been baked — the same gap `ListVmImages` filled for built images.
