# tddy-vm: General-purpose VM build and lifecycle management

**Product area:** VM
**Updated:** 2026-06-21
**Status:** In development

## Summary

`tddy-vm` is a new package that provides a **general-purpose VM build and lifecycle capability** for the tddy-coder workspace. It decouples VM management from the "demo goal" concept and exposes it as a first-class, session-independent capability via:

- A **standalone VM registry** — define VMs by name (with image path or build target), list them, start/stop/remove them independently of any session or demo-plan.md.
- A **build wrapper** — invoke the existing `tddy-build-qemu` / `tddy-build-buildroot` plugins to produce qcow2 images from BUILD.yaml targets.
- An **RPC service** (`vm.VmService`) exposed over the daemon's HTTP and LiveKit transports, discoverable via gRPC reflection.
- A **`/vms` page** in the web dashboard — list, start, stop, and remove VMs from the UI.

## Relationship to the demo goal

The existing demo-goal infrastructure (`tddy-demo-runner`, `StartDemoVm`/`StopDemoVm` RPCs) continues to work unchanged externally but is now a thin layer over `tddy-vm`. The `DemoOrchestrator` uses `tddy_vm::Vm` (the generalised trait) instead of its own `DemoVm` copy. The daemon's session-scoped `StartDemoVm` RPC also uses `tddy_vm::QemuVm` for the actual QEMU process.

This means there is a single source of truth for VM lifecycle logic, and the demo goal gains robustness from improvements to the general VM layer.

## VM Registry

VMs are defined with a `VmSpec`:

| Field | Description |
|-------|-------------|
| `name` | Unique identifier |
| `build_target` | Optional — BUILD.yaml target ID to build the qcow2 image |
| `image_path` | Optional — path to an existing qcow2 image (mutually exclusive with `build_target`) |
| `port_forwards` | `Vec<PortForward { host_port, guest_port }>` — slirp hostfwd mappings beyond SSH |
| `ssh_host_port` | Host-side SSH port (default 2222; must be unique across running VMs) |

Specs are persisted to a JSON file in the daemon's data directory so definitions survive daemon restarts.

`VmState` lifecycle: `Defined → Booting → Running → Stopped` (or `Error`).

## RPC surface

New `vm.VmService` with 7 methods:

| Method | Description |
|--------|-------------|
| `BuildVmImage` | **Server-streaming.** Accept a Buildroot `.config` spec, invoke Buildroot directly (via Nix), stream progress lines, emit final message with qcow2 path or error. Independent of tddy-build. (Updated: 2026-06-21) |
| `DefineVm` | Register a VM spec in the registry |
| `ListVms` | List all VMs with their current state |
| `StartVm` | Boot a named VM (builds image first if `build_target` set) |
| `StopVm` | Graceful shutdown via QEMU monitor `system_powerdown` |
| `GetVmStatus` | Current state + SSH port + share URL |
| `RemoveVm` | Remove a stopped VM from the registry |

All methods require `session_token` for authentication (same pattern as `ConnectionService`).

## Web UI

A `/vms` page in the dashboard (accessed via the hamburger nav menu). (Updated: 2026-06-21)

### Build image panel

A two-step panel for building a disk image and creating a VM: (Updated: 2026-06-21)

1. **Buildroot spec textarea** — the user pastes a Buildroot configuration spec (e.g. a defconfig name or config fragment). Clicking **Build image** sends the spec to a backend that invokes Buildroot **directly** — this is completely independent of the tddy-build graph system and repo-internal build targets. A "Building…" indicator shows while the build is in progress (Buildroot builds can take 20–60 min on first run). Errors surface inline below the textarea.

   > ⚠️ **Not yet implemented on the backend.** The current `BuildVmImage` RPC incorrectly passes the textarea content as a tddy-build target ID. A new backend path is required (Updated: 2026-06-21):
   > - **Spec format:** full Buildroot `.config` content (same `BR2_*` variable syntax as Linux kernel config). Example minimal QEMU x86_64 image: `BR2_x86_64=y`, `BR2_TOOLCHAIN_BUILDROOT_GLIBC=y`, `BR2_TARGET_ROOTFS_EXT2=y`, `BR2_LINUX_KERNEL=y`, etc.
   > - **Buildroot source:** provided by Nix (`buildroot` added to the Nix flake). No hardcoded path — the daemon finds it via the Nix environment.
   > - **Build flow:** write spec to temp workspace as `.config` → `make olddefconfig` → `make -j$(nproc)` → `qemu-img convert rootfs.ext4 → output.qcow2` → stream qcow2 path in the final message.
   > - **Streaming:** `BuildVmImage` is a **server-streaming RPC** — it emits a sequence of progress messages as the build runs (Buildroot stdout lines forwarded in real time), with a final message carrying the result or error. The UI renders these as a live build log below the textarea. Both transports support server streaming natively: the LiveKit transport (`tddy-livekit-web`) uses an `AsyncQueue` fed by data-channel messages, and the HTTP transport uses HTTP/2 streaming. No transport-level workarounds needed. (Added: 2026-06-21)

2. **Create VM form** — once an image is available, a **dropdown** lists all successfully built image paths. The user selects an image, enters a VM name, and clicks **Create VM** to call `DefineVm`. The dropdown accumulates images across multiple builds in the same session.

### VM table

A table of all defined VMs with their state, SSH host port, share URL, and **Start / Stop / Remove** action buttons. The table refreshes after every mutating action (define, start, stop, remove).

## Architecture

```
tddy-vm (new)
├── vm.rs           — Vm trait (mockable boundary), VmConfig, RunningVm, VmError, PortForward
├── qemu.rs         — QemuVm (full impl), QemuVmArgs (pure arg builder), wait_for_ssh_port, send_monitor_command
├── mock.rs         — MockVm (recording test double)
├── build.rs        — build_vm_image() — currently wraps tddy-build (WRONG for spec-based builds; needs new direct-Buildroot path)
├── cloud_init.rs   — image-chaining argv/document builders + build_cloud_init_image orchestrator;
│                     cloud_init_library_paths maps a build's outputs into the VM & Image Library
├── library.rs      — VmLibrary: images/01-base, images/02-prepared-base, vm/<name>/ layout;
│                     init, import_base_image, write/read/list_manifests, remove_vm, create_vm;
│                     set_readonly_file, vm_overlay_create_argv (absolute-backing overlay argv)
├── vm_manifest.rs  — VmManifest, RunPolicy, LoginPolicy (per-VM manifest.yaml)
├── registry.rs     — VmSpec, VmState, VmManager (Storage::Json — HashMap + JSON persistence —
│                     or Storage::Library — VmLibrary-backed, source of truth going forward)
└── service.rs      — VmServiceImpl (implements generated vm::VmService trait)

tddy-vm-build
└── src/lib.rs — `build` (Buildroot spec) + `cloud-init` (image-chaining, via VmLibrary) subcommands

tddy-service
└── proto/vm.proto — VmService definition → generates Rust + TypeScript clients

tddy-daemon (repointed)
└── main.rs — VM service construction builds a VmLibrary at the resolved data root and
    constructs VmManager::from_library, instead of the old vm-registry.json-backed VmManager::new

tddy-demo-runner (refactored)
└── orchestrator.rs — DemoOrchestrator uses tddy_vm::Vm; vm.rs/qemu.rs/mock.rs deleted

tddy-web
└── src/components/vms/
    ├── VmsAppPage.tsx   — container: RPC wiring, state (building, availableImages)
    ├── DefineVmPanel.tsx — presentational: spec textarea + image dropdown + Create form
    └── VmsScreen.tsx    — presentational: VM table with Start/Stop/Remove
```

## Requirements

1. `tddy-vm` package compiles cleanly with stub implementations.
2. `vm.proto` defines all 7 RPCs with correct message types.
3. `VmServiceServer` is registered in `tddy-daemon` and appears in gRPC reflection.
4. `tddy-demo-runner` has no duplicated VM lifecycle code.
5. `/vms` page renders in the web app and appears in the nav menu.
6. `VmManager` persists specs to JSON; serde round-trip is correct.
7. `build_vm_image` invokes the tddy-build system (QemuPlugin + BuildrootPlugin registry). [SUPERSEDED — see below]

## Image builder CLI (`tddy-vm-build`) (Added: 2026-07-01)

**Product area:** VM
**Feature PRD:** this section
**Status:** Implemented — verified against two real (non-mocked) Buildroot builds on macOS via the Docker toolchain below (`#[ignore]`+`#[serial]`, ~62 min total); see [packages/tddy-vm/docs/changesets.md](../../../packages/tddy-vm/docs/changesets.md) and [packages/tddy-vm-build/docs/changesets.md](../../../packages/tddy-vm-build/docs/changesets.md)

A standalone binary that builds a VM image from a Buildroot `.config` spec and writes it
to an explicit output file, independent of the daemon/RPC path:

```
tddy-vm-build --spec <path-to-.config> --output <path> --format qcow2|raw
```

- **Spec format:** same Buildroot `.config` (`BR2_*`) syntax as the existing
  `BuildVmImage` RPC (§ RPC surface above).
- **Output:** `--format qcow2` (default) runs `qemu-img convert` as today; `--format raw`
  writes the Buildroot rootfs image unconverted.
- **Core logic is shared, not duplicated:** the CLI calls a new pure
  `tddy_vm::build::build_image(spec, output, format, progress)` function. The existing
  `build_vm_image_from_spec` (used by the `BuildVmImage` RPC) is refactored to call the
  same core with its gRPC progress channel as the `progress` sink — no behavior change to
  the RPC path.
- **Requires `BUILDROOT_DIR`** in the environment, exactly as the RPC path does today.
- **macOS builds route through Docker.** Buildroot's own dependency checker
  (`support/dependencies/dependencies.sh`) rejects Apple Clang's `gcc` trampoline and
  expects several Linux-only host tools. On macOS, `run_buildroot_pipeline` (shared by
  `build_image` and `build_vm_image_from_spec`) transparently runs `make olddefconfig`/
  `make -j<nproc>` inside a small Linux container instead of natively, building the image
  from `packages/tddy-vm/docker/buildroot-host/Dockerfile` on first use (cached
  thereafter via `docker image inspect`). `BUILDROOT_DIR`/the download cache/the build
  tree are bind-mounted (not copied) so the produced image lands at the same host path
  either way. Override via `TDDY_VM_BUILD_TOOLCHAIN=native|docker`; every non-macOS host
  defaults to `native`. Requires Docker to be installed and running — already a repo
  dependency via `tddy-build-docker`.

## VM testkit — build-in-a-VM and test-in-a-VM (`tddy-vm-testkit`) (Added: 2026-08-14)

**Product area:** VM / Sandbox / Supervisor
**Status:** Implemented; production tests written but not yet run end to end
**Changeset:** [docs/dev/1-WIP/vm-cgroups-testkit.md](../../dev/1-WIP/vm-cgroups-testkit.md)

A testkit that makes the workspace's Linux-only code testable from a macOS host, by
building it in one guest and asserting against it in another.

**The image chain**, cached under the repo's `tmp/.tddy` (the same dev data dir `./web-dev`
uses), all derived from one cloud image supplied on disk. (Updated: 2026-08-14 — reworked
from flattened copies to a true backing chain.)

```
images/01-base/<supplied>.qcow2   the ONLY full image — sealed copy of the pristine
                                  download, no backing file, 0444
  ↑ absolute backing
images/02-prepared-base/          ← one flat dir; links within it are bare basenames
  tddy-nix-base.qcow2             delta: Nix + flakes, tddy + alice accounts
    ↑ relative basename backing
  tddy-builder.qcow2              delta: + 9p kernel, + warmed dev shell
  tddy-test-host.qcow2            delta: + tddy-clients group, stock kernel
```

**Chaining discipline.** The imported pristine image is the **only full image in the
system**. Every layer above it — the cloud-init one included — is a true delta created with
`qemu-img create -f qcow2 -F qcow2 -b <parent> <child> <size>`, and **each provisioned
overlay becomes the backing file of the next layer**: never flattened, never committed down.
A `qemu-img convert` runs only to normalise a non-qcow2 supplied image, once, at import.

Path style follows one rule: *same-directory link ⇒ bare basename (with `cwd` set to that
directory); cross-directory link ⇒ absolute path to the durable location.* Only the
`02-prepared-base/` → `01-base/` link is cross-directory.

This mirrors `~/Code/makers-lt`'s `@wix/maker-vm` (`maker-build/maker-vm`), the same
reference the VM & Image Library's own chaining was modelled on.

Because every layer is a delta, the shared Nix parent saves both bake time *and* disk.

> `cloud_init::build_cloud_init_image` implements this for every layer alike: it chains a
> delta directly onto whatever image it is given, whether that is the pristine import or an
> already-provisioned layer. The backing reference is computed by `relative_backing_path`,
> and the overlay is created at its final path — a relative reference resolves against the
> directory holding the image, so a created-then-moved overlay would point at nothing.

**The builder guest** is on the critical path, not an optimisation: `tddy-supervisor`,
`tddy-daemon` and `tddy-sandbox-runner` must be Linux/aarch64 ELF binaries, and an
Apple-Silicon host cannot emit those. It mounts the working copy read-only over 9p and
`tmp/.tddy/dist` **writable** (the first writable 9p share in the workspace), runs
`./release`, and hands the binaries back to the host. Its overlay is long-lived so
`/opt/tddy/target` survives and rebuilds are incremental.

**The test host** gets a fresh, disposable overlay per run — cgroup state must never be
inherited — receives the binaries by `scp`, and installs them with the real
`./install --systemd --headless`. It keeps Debian's stock `-cloud` kernel: giving it the
generic flavour for 9p would diverge the kernel under test from the one a real host runs,
and the thing under test *is* kernel behaviour. That is why binaries arrive by scp, and why
`tddy-vm` grew `scp_opts`/`scp_to_guest` (note `scp` spells the port `-P`, not `-p`).

**What it unlocks.** Before this, every function touching `/sys/fs/cgroup` was either never
executed by a test or exercised against a `tempfile::tempdir()` — which accepts any bytes,
enforces no limit, and returns `ENOTEMPTY` forever where the kernel returns `EBUSY`, so
scope removal's retry path and success path had never run. The production tests in
`packages/tddy-e2e/tests/vm_cgroups_acceptance.rs` assert delegation the kernel actually
honoured, `EBUSY` on a populated scope, `pids.max` refusing a fork, and a root supervisor
with an already-dropped daemon.

**Running it.** `#[ignore]` + env-gated, so `./test` is unaffected. `./run-vm-testkit`
warms the cache; `./run-vm-testkit --status` reports what is cached.

## QEMU sandbox backend (`tddy-sandbox-qemu`) (Added: 2026-07-01)

**Product area:** VM / Sandbox
**Feature PRD:** this section (backend contract defined in `tddy-sandbox`, see
`packages/tddy-sandbox/src/builder.rs`)
**Status:** Implemented — real overlay creation, QEMU boot, and in-guest `tddy-sandbox-runner`
handshake over a forwarded TCP control port (reusing the existing `run_host_relay`, no
changes needed to shared darwin/cgroups infrastructure); `tddy-daemon` backend selector
wired. Guest-side 9p mount + init hook documented with real artifacts (see
[packages/tddy-sandbox-qemu/docs/guest-image-9p-init.md](../../../packages/tddy-sandbox-qemu/docs/guest-image-9p-init.md))
but not yet exercised inside an actual booted guest — see "Known gaps" below.

A CLI binary and library backend that boots a qcow2 image built above and runs it as a
full `tddy-sandbox` confinement backend — the same `SandboxPlan` contract implemented by
`tddy-sandbox-darwin` (Seatbelt) and `tddy-sandbox-cgroups` (Linux namespaces), except the
confinement boundary is a QEMU VM instead of a host-level jail:

```
tddy-sandbox-qemu --image <qcow2> \
  --mount <host-dir>:<jail-path>[:rw] \
  --env KEY=VALUE \
  --cwd <guest-path> \
  -- <command...>
```

- **Host directory mounts** are the headline capability requested for this backend:
  each `SandboxPlan` `MountSpec` becomes a virtio-9p share (`-fsdev local` +
  `-device virtio-9p-pci`), read-only unless `writable`. This requires the guest image to
  enable 9p in its Buildroot config — see
  [packages/tddy-sandbox-qemu/docs/guest-image-9p-init.md](../../../packages/tddy-sandbox-qemu/docs/guest-image-9p-init.md)
  for the kernel fragment + init hook.
- **Everything the sandbox builder supports** (reads, copies, symlinks, env, secrets,
  network policy, resource limits, PTY) flows through the same `SandboxPlan` the darwin
  and cgroups backends consume — see `packages/tddy-sandbox/src/builder.rs` for the full
  model. The in-guest counterpart to the darwin/cgroups jail is
  `tddy-sandbox-runner` (already platform-agnostic), injected into the guest via a
  reserved 9p share plus a small init hook.
- **Image selection is out-of-band:** `SandboxPlan` itself carries no VM-image field
  (that model is shared with the non-VM backends); the image path is a CLI flag /
  backend option (`QemuBackendOptions`), not a plan field.
- **Daemon integration:** `tddy-daemon` gains a backend selector (env or config, since
  QEMU is not `target_os`-gated like darwin/cgroups) to route `spawn_sandbox_runner` to
  `tddy_sandbox_qemu::spawn_plan` — additive, opt-in, existing backends remain default.

## Cloud-init image build with image-chaining (`tddy-vm-build cloud-init`) (Added: 2026-07-02)

**Product area:** VM
**Feature PRD:** this section
**Status:** Implemented — unit tests (22) pass; real QEMU boot verified to succeed
end-to-end multiple times in a nested-virtualization sandbox, including one real bug
found+fixed (a benign `set_hostname` module failure misclassified as fatal) and one
boot-speed fix (`ds=nocloud` SMBIOS pinning) — see
[packages/tddy-vm/docs/changesets.md](../../../packages/tddy-vm/docs/changesets.md)
and [docs/dev/changesets.md](../../dev/changesets.md) for details. The
`#[ignore]`+`#[serial]` real-boot acceptance tests remain timing-sensitive under that
sandbox's fixed budgets due to environment resource contention, not implementation
defects.

A second `tddy-vm-build` subcommand, alongside `build` (renamed from the previous flat
invocation — see Migration note below), that provisions a cloud-init cloud image instead
of running a Buildroot build:

```
tddy-vm-build cloud-init \
  --name <image-name> \
  --base-image <path-to-a-real-cloud-image>   # or set TDDY_CLOUDINIT_BASE_IMAGE
  --output-dir <dir> \
  --user-data <cloud-init-user-data.yaml> \
  --disk-size 20G --memory 2048M --cpus 2 --ssh-host-port 2222 \
  [--ssh-public-key <path>] [--timeout-secs 300]
```

`--base-image` (or the `TDDY_CLOUDINIT_BASE_IMAGE` env var) is the only way to point
this feature at a base image — there is no bundled or auto-downloaded default, and no
machine-specific path is baked into the CLI. Reuses this repo's existing QEMU
primitives instead of duplicating them:

- **Chained delta overlay.** (Updated: 2026-08-14 — the base is no longer copied or
  converted.) The caller-provided base image is used **in place** as the backing file: it
  is never downloaded, never re-fetched and never mutated. A delta overlay is created
  directly at its final path with `qemu-img create -f qcow2 -F qcow2 -b <relative-parent>
  <overlay> <disk-size>`, the backing reference computed by `relative_backing_path` so the
  whole library relocates as a unit. The product of a bake is **one file**, not a pair.
  Because a relative reference resolves against the directory holding the referencing
  image, the overlay is created where it will live and is never moved afterwards. Mirrors
  the existing
  ephemeral-overlay primitive in `tddy-sandbox-qemu`'s `overlay_create_argv`
  (`packages/tddy-sandbox-qemu/src/argv.rs`), adapted for disk sizing and relative
  basenames.
- **NoCloud cloud-init seed.** User-data/meta-data are rendered to a `seed/nocloud/`
  directory and packed into a `cidata`-labeled ISO9660 image (Joliet + Rock Ridge) via
  `xorriso -as mkisofs` (mkisofs-emulation mode — no new Rust ISO dependency). The
  `{{SSH_PUBLIC_KEY}}` placeholder in `ssh_authorized_keys` is replaced with either a
  caller-supplied key (`--ssh-public-key`) or a freshly generated keypair
  (`ssh-keygen`). The rendered `users:` list is led by the bare string `default`, keeping
  the distro's own account — a `users:` key *replaces* it, and `cc_ssh_authkey_fingerprints`
  still resolves it by name, failing `cloud-final` with
  `KeyError: getpwnam(): name not found: 'debian'` when it is missing (Added: 2026-08-14).

  **Nothing is injected into `bootcmd`** (Updated: 2026-08-14). What stops a pre-baked cloud
  image's prior cloud-init state from suppressing re-provisioning is the seed's own
  `instance-id`, which names the layer being built and is therefore one cloud-init has never
  processed. A `cloud-init clean --logs --seed` `bootcmd` used to be injected for that and
  was actively destructive: `clean` deletes `/var/lib/cloud/instance/`, where the config
  stage writes the `runcmd` script the final stage is about to run, so the boot it ran on
  provisioned nothing and the host sealed an empty image and reported success. A step that
  must reboot mid-bake resets the instance state itself, on its way out
  (`cloud_init::reset_cloud_init_and_reboot`).
- **Bake-in by booting.** The overlay is booted with the seed ISO attached
  (`-cdrom`), reusing `QemuVmArgs`' argv shape (`packages/tddy-vm/src/qemu.rs`) with the one
  difference needed to observe completion: `-serial stdio` (not `file:`).

  **The bake deliberately does *not* pass `-no-reboot`** (Updated: 2026-08-02). Under that
  flag QEMU *exits* on a guest reset, so any provisioning step that reboots — the tddy host
  recipe's kernel swap below does — ends the emulator, the serial reader hits EOF, and the
  bake fails with "exited before the cloud-init completion token was observed" before doing
  any real work. `-no-reboot` was never needed to end the process on success either: the
  completion script's `shutdown -h now` is a *power-off*, which terminates QEMU on its own.
  The cost of dropping it is that a guest stuck in a boot loop is caught by the build timeout
  rather than by an immediate EOF; once the token *is* seen, the process is given a bounded
  grace period to halt and is force-killed if it resets instead.

  A deterministic completion token
  (`CLOUDINIT_COMPLETE_<name>_<sha256(provisioning-input)[:12]>`) is embedded in
  user-data as the **last `runcmd` step**, which dumps the guest's cloud-init logs to the
  console, prints the token and calls `shutdown -h now`; an EXIT trap armed by the **first**
  `runcmd` step prints `<token>_FAILED` (and halts) if any earlier step exits non-zero.
  The host watches the serial stream line-by-line (reusing
  the `BufReader`/`tokio::select!` draining pattern from `build.rs::run_parallel_build`)
  and returns once the token is observed, with `send_monitor_command`
  (`packages/tddy-vm/src/qemu.rs`) as a graceful-shutdown fallback on timeout. The
  overlay this produces is fully provisioned — no first-boot cloud-init step needed by
  the consumer.

  **The token used to be a `scripts-per-boot` script, and that was a real bug** (Fixed:
  2026-08-14). `cloud_final_modules` runs `scripts-per-boot` *before* `scripts-user`, so
  the guest halted — and the host sealed the overlay and reported success — before `runcmd`
  had run at all: a bake of a recipe installing six apt packages and Nix produced a 21M
  delta in 0.46s of "provisioning", its boot log showing no `scripts-user`, no `runcmd`,
  and `set_hostname` as the only module that ran. Nothing but `runcmd` runs after `runcmd`,
  which is why the signal now lives there.
- **New module `tddy_vm::cloud_init`** — all argv/document-rendering logic is exposed
  as pure, unit-testable builder functions (`relative_backing_path`, `overlay_create_argv`,
  `render_user_data`, `render_meta_data`, `completion_token`, `seed_iso_argv`,
  `iso_tool_command`, `cloud_init_boot_argv`, `classify_serial_line`), composed by an
  async orchestrator `build_cloud_init_image`.

### Migration note

Introducing this subcommand requires `tddy-vm-build` to gain a `Cli { #[command(subcommand)] }`
wrapper with `build` and `cloud-init` variants. The previous flat invocation
(`tddy-vm-build --spec … --output … --format …`) becomes `tddy-vm-build build --spec … --output … --format …`.

### Production tests (manual trigger only)

The real-QEMU-boot tests (`packages/tddy-vm/tests/cloud_init_acceptance.rs`,
`packages/tddy-vm-build/tests/cloud_init_cli_acceptance.rs`) are production tests per
[docs/dev/guides/testing.md](../../dev/guides/testing.md#production-tests): `#[ignore]`d
(excluded from `./test`/`./verify`/plain `cargo test`) *and* gated on
`TDDY_CLOUDINIT_BASE_IMAGE` pointing at a real cloud-init-compatible qcow2 image — the
same config the CLI's `--base-image` reads. They do not run at all, even with
`--ignored`, unless a developer explicitly supplies that env var; there is no bundled
or auto-discovered image.

### Out of scope for this sub-feature

- Downloading the base cloud image (the base is always caller-supplied/copied).
- Flattening the chained pair into a single standalone qcow2 (the delta-overlay model is
  the explicit goal).
- Non-Debian/non-cloud-image bases, multi-NIC network-config beyond DHCP, or a
  persistent registry of cloud-init-built images (the existing `ListVmImages` gap
  applies here too — see Known gaps below).

## VM & Image Library (Added: 2026-07-02)

**Product area:** VM
**Feature PRD:** this section
**Status:** Implemented and verified — 65 unit/acceptance tests pass in `tddy-vm`
(0 failed, 3 correctly-gated `#[ignore]`d), `clippy -D warnings` clean across `tddy-vm`,
`tddy-vm-build`, and `tddy-daemon`, daemon repointed. `VmLibrary::create_vm`'s real
`qemu-img create` overlay was additionally run against a real prepared base (makers-lt's
`debian-12-base.qcow2`) and confirmed via `qemu-img info` to record the expected
absolute backing-file path. The `tddy-vm-build cloud-init` CLI wiring compiles and its
4 production tests (split by semantic claim: produces a valid chained pair, imports the
raw base, locks both halves read-only, keeps scratch artifacts out of the flat
`02-prepared-base/`) are correctly updated/gated, but could not be run end-to-end in
this environment (no `xorriso`/`mkisofs`/`genisoimage` on PATH for the seed-ISO step).
See [packages/tddy-vm/docs/changesets.md](../../../packages/tddy-vm/docs/changesets.md),
[packages/tddy-vm-build/docs/changesets.md](../../../packages/tddy-vm-build/docs/changesets.md),
and [docs/dev/changesets.md](../../dev/changesets.md) for the full delta.

Organizes base images, prepared bases, and per-VM state under a single **library**
rooted at the existing tddy data dir (the same root `tddy-daemon` already resolves via
`default_tddy_data_dir()`/`tddy_data_root_matching_child()` — no new env var):

```
<tddy_data_dir>/
  images/
    01-base/            immutable base images, downloaded from the internet   (files chmod 0444)
    02-prepared-base/   read-only, cloud-init-baked prepared bases            (files chmod 0444)
  vm/
    <vm-name>/
      manifest.yaml     how to run, login policy, SSH keys, prepared-base reference
      <name>.qcow2      mutable overlay backed by a prepared base
      id_<name>[.pub]   SSH keypair for login (private key chmod 0600)
```

Design mirrors `~/Code/makers-lt`'s `maker-vm` package: image chaining is pure qcow2
backing files (pristine import → cloud-init overlay → further overlays → per-VM mutable
overlay), reusing this crate's existing `cloud_init` argv builders
(`relative_backing_path`, `overlay_create_argv`) rather than reinventing them. (Updated:
2026-08-14 — the pristine import is the only full image; there is no flattened
intermediate.)

- **`VmLibrary`** (`packages/tddy-vm/src/library.rs`) — path accessors for the layout
  above; `init()` creates the tree; `import_base_image` copies a base into `01-base` and
  locks it read-only; `write_manifest`/`read_manifest`/`list_manifests`/`remove_vm` manage
  per-VM manifest files; `create_vm` builds a per-VM overlay from a named prepared base
  and writes the manifest + SSH keys. `vm_overlay_create_argv` builds the per-VM overlay's
  `qemu-img create` argv using an **absolute** backing-file path (the overlay lives in
  `vm/<name>/`, separate from the read-only `02-prepared-base/` its prepared base lives
  in) — contrast `cloud_init::overlay_create_argv`'s co-located **relative** basename.
- **`VmManifest`** (`packages/tddy-vm/src/vm_manifest.rs`) — the per-VM manifest, in
  YAML: `name`, `prepared_base` (name of an image in `02-prepared-base`) or `image_path`
  (an existing, library-unmanaged qcow2 — mutually exclusive, mirrors `VmSpec`'s existing
  `build_target`/`image_path` duality), a `RunPolicy` (memory, cpus, disk size, SSH host
  port, port forwards), and a `LoginPolicy` (SSH username + key paths).
- **`VmManager` becomes library-backed** — `VmManager::from_library(library, backend)` is
  a new constructor alongside the existing JSON-backed `VmManager::new`; per-VM
  `manifest.yaml` files are the source of truth for VMs created this way, superseding the
  single shared `vm-registry.json` for the daemon's own wiring. `VmSpec` remains the
  in-memory/RPC DTO — the existing `VmService` RPC surface and web UI are unaffected;
  `VmManager` maps between `VmSpec` and `VmManifest` internally.
- **Cloud-init wiring** — `cloud_init_library_paths` (in `tddy_vm::cloud_init`) resolves a
  cloud-init build's outputs into the library: the supplied input base into `01-base/`,
  and the single provisioned overlay into `02-prepared-base/`, chained onto it by a
  relative backing reference. (Updated: 2026-08-14 — there is no second, flattened half;
  the overlay is created at this final path and never moved, because a relative reference
  resolves against the directory holding the image.) `tddy-vm-build
  cloud-init` points the existing `build_cloud_init_image` pipeline at a per-image scratch
  subdirectory, `02-prepared-base/<name>/`, so every artifact it produces (seed ISO,
  `seed/nocloud/` sources, generated SSH keypair, boot log) lands there; once baking
  succeeds, only the finished qcow2 pair is moved out to the flat `02-prepared-base/`
  location (both files together, so the overlay's relative backing reference to the base
  stays valid), leaving the scratch artifacts behind in the subdirectory instead of
  cluttering `02-prepared-base/` with non-image files.
- **Filesystem protection** — files placed into `01-base`/`02-prepared-base` are chmod
  `0o444` (read-only) via `set_readonly_file`; the two directories stay `0755`. No
  download of any image is performed by this feature — tests reuse an already-built base
  image supplied via `TDDY_CLOUDINIT_BASE_IMAGE` (e.g. makers-lt's `debian-12-base.qcow2`).

### Out of scope for this sub-feature

- Deleting the JSON-backed `VmManager::new`/`vm-registry.json` code path — it remains
  available; only the daemon's own construction is repointed at the library.
- RPC/proto changes or web UI changes — backend-only.
- Downloading any base image.

### Known gaps — VM & Image Library (Added: 2026-07-02)

- **The `tddy-vm-build cloud-init` production test was not run end-to-end** in the
  environment this feature was implemented in (missing ISO tooling) — only compiled and
  gating-verified. The equivalent `tddy-vm`-level `create_vm` production test was run for
  real, against a real prepared base.

## Daemon-spawned tddy host VM (Added: 2026-08-02)

**Product area:** VM
**Feature PRD:** this section
**Status:** Implemented. All six boot-control acceptance tests pass against a real
`debian-12-genericcloud-arm64` guest on an Apple Silicon host (~327 s, `--test-threads=1`),
and 167 unit/integration tests pass in `tddy-vm` with 19 more in the daemon's VM suites.
**The hours-long bake itself has not yet been run end to end** — see "Running the bake" below
for how, and "Known gaps" for what that leaves unproven.

Lets the daemon spawn a VM that *is itself a tddy host*: a Debian cloud image baked by
cloud-init into a prepared base that has `tddy-daemon`/`tddy-coder`/`tddy-tools` built from
the operator's own working copy and installed as a systemd service, with the guest daemon
configured to join the LiveKit common room. Once such a VM is running, **project cloning and
session start are the existing flow** — the guest announces itself via the existing peer
discovery, and `CreateProject` / `StartSession` target it by `daemon_instance_id`. No new
repo-provisioning code is introduced.

### User story

As a developer, I want the daemon to spawn a VM that runs my current build of tddy, so I can
start sessions against a disposable, isolated host without provisioning a machine by hand.

### RPC surface — two new methods on the existing `vm.VmService`

The split mirrors the library's `01-base → 02-prepared-base → vm/<name>` layering: the
expensive bake happens **once**, and many cheap VMs are created from its output.

| Method | Description |
|--------|-------------|
| `BuildTddyHostImage` | **Server-streaming.** Import the caller-supplied Debian cloud image into `images/01-base/`, render a tddy-host cloud-init user-data document, bake it by booting with the operator's working copy attached over virtio-9p, and emit the finished prepared-base name. Streams every serial-console line as progress. |
| `CreateVmFromPreparedBase` | Create `vm/<name>/` from a named prepared base: mutable overlay, generated per-VM SSH keypair, and `manifest.yaml` carrying the `RunPolicy` (arch, accel, memory, cpus, disk, ports) and `LoginPolicy`. |

`StartVm` / `GetVmStatus` / `StopVm` / `RemoveVm` keep their existing signatures and cover the
rest of the lifecycle. Both new methods require `session_token`, like every other method on
this service.

**No `tddy-web` changes in this changeset** — the RPC surface only. The `/vms` page continues
to work against the unchanged methods.

### Cloud-init bake: what the guest does

The rendered user-data provisions in this order:

1. Mounts the operator's working copy from the reserved virtio-9p share (`mount_tag`
   `tddy-src`, read-only) at `/mnt/tddy-src` and copies it to `/opt/tddy` — so the image is
   built from exactly the tree on the operator's disk, with no git URL and no credentials.
2. Installs the Nix package manager, because `./release` and `./install` are defined in terms
   of the repo's nix dev shell. This is the documented build path; bypassing it with
   `apt`+`rustup` would silently drop toolchain the flake provides.
3. Runs `./release` (Rust binaries) and `bun run build` (the web bundle `./install` copies).
4. Runs `./install --systemd`, producing the `tddy-daemon.service` + `tddy-daemon.socket`
   units the repo already ships.
5. Leaves a daemon config at the install config path containing the LiveKit `common_room`
   settings, written via cloud-init `write_files` **before** `runcmd` — `./install` keeps an
   existing config rather than overwriting it, so the LiveKit wiring survives the install.

### QEMU launcher corrections (prerequisites)

`QemuVmArgs::build` currently hard-codes `qemu-system-x86_64`, `-m 512M`, no `-smp`, no
accelerator, and no firmware. None of that can boot a Debian arm64 cloud image on an Apple
Silicon host, so this changeset makes the launcher honour the manifest:

- **Architecture** — `VmArch::{Aarch64, X86_64}` selects the `qemu-system-*` binary and the
  machine type (`virt` for aarch64, `q35` for x86_64).
- **Acceleration** — `VmAccel::{Hvf, Kvm, Tcg}`, emitted as `-machine <type>,accel=<accel>`
  with `-cpu host` when hardware-accelerated. `VmAccel::host_default()` is an explicit
  constructor used to populate the manifest; the launcher itself never guesses.
- **Resources** — `RunPolicy.memory` and `RunPolicy.cpus` reach `-m` / `-smp` (they are
  currently written to `manifest.yaml` and then ignored).
- **UEFI** — aarch64 `virt` has no legacy BIOS, so the launcher emits a two-pflash pair: the
  read-only `edk2-<arch>-code.fd` firmware and a writable 64 MiB vars file per VM. The
  firmware is resolved from `TDDY_VM_UEFI_CODE`, else from the QEMU installation's
  `share/qemu/` directory; **an unresolvable firmware path is an error, not a silent
  fallback to BIOS boot.**
- **Login** — SSH uses `LoginPolicy.username` and `-i <ssh_private_key>` from the per-VM
  keypair, replacing the current unconditional `root@` with the ambient agent key.

Verified on the target host: the Debian 12 arm64 genericcloud image reaches a login prompt in
roughly 17 seconds under `-machine virt,accel=hvf -cpu host` with `edk2-aarch64-code.fd`.

### Serial-console control (`tddy_vm::serial_shell`)

Baking and verifying a VM both need to drive the guest over its serial console — before
sshd, networking, or cloud-init are necessarily up. Today the crate can only match a single
completion token (`classify_serial_line`); there is no way to log in or run a command over
UART.

This changeset adds a console driver modelled on `~/Code/makers-lt/common/shell-utils`
(`ShellHandler`), ported to Rust and restructured so the parsing core is pure:

- `SerialShellState` — `Prelude → AtLogin → AtPassword → AtPrompt → ExecutingCommand`.
- `SerialShell::feed(&mut self, chunk: &str) -> Vec<SerialShellEvent>` — a **pure** state
  machine over a byte stream, handling partial lines and prompt-without-newline. Returning
  events instead of the TypeScript version's `EventEmitter` is what makes it unit-testable
  with no VM and no I/O.
- `strip_ansi_codes` — pure, shared by prompt detection.
- Configurable login/password/command prompt patterns, plus optional auto-login credentials.
- `SerialConsole` — the async driver that owns the QEMU serial pipe and exposes
  `wait_for_prompt`, `login`, and `run_command`.

One deliberate departure from the TypeScript original: `run_command` determines completion
from an explicit exit-code marker (`; echo <MARKER>$?`) rather than from the next prompt
match. The original's `sendCommand` resolves on the following `prompt` event and emits a
synthetic `prompt` to start the queue, which cannot distinguish "command finished" from "a
prompt-shaped line appeared in the output" and yields no exit status.

For context on how the same problem is solved upstream: `maker-vm`'s cloud-init boot script
detects completion by scanning console lines for a unique injected token plus sentinel
banners, and treats reaching a `login:` prompt without the token as a build failure. This
crate already has the token half (`classify_serial_line`); `serial_shell` adds the login and
command-execution half, so the bake can also *interrogate* a guest that failed rather than
only reporting that the token never arrived.

### Acceptance criteria

1. `BuildTddyHostImage` with a valid token and a real Debian cloud image produces a prepared
   base in `images/02-prepared-base/`, and streams serial-console output as progress.
2. A VM created from that prepared base boots, and `systemctl is-active tddy-daemon` in the
   guest reports `active`.
3. The guest daemon's Connect port answers over the forwarded host port.
4. `CreateVmFromPreparedBase` writes `manifest.yaml`, a mutable overlay backed by the
   prepared base, and a per-VM SSH keypair; the public key is returned to the caller.
5. SSH into a started VM succeeds as `LoginPolicy.username` using the generated private key.
6. The serial console reaches a login prompt, accepts credentials, and executes a command
   returning its real exit code — with no SSH and no guest networking.
7. The read-only 9p share is mounted in the guest and its contents are readable.
8. Both new methods reject an invalid `session_token` with `unauthenticated`.
9. `StopVm` shuts a running VM down gracefully via the QEMU monitor.

### Testing

Per the developer's direction, this feature is only meaningfully proven by an end-to-end run
against a real VM with demonstrated control of it. Acceptance tests are therefore real-boot
[production tests](../../dev/guides/testing.md#production-tests): `#[ignore]` + `#[serial]`
*and* gated on an env var pointing at a real Debian cloud image, exactly like
`cloud_init_acceptance.rs`. They are excluded from `./test`, `./verify`, and plain
`cargo test`, and do not run even with `--ignored` unless the image variable is set.

The pure builders (argv assembly, user-data rendering, serial-shell state machine) also carry
ordinary unit tests, and the two new RPCs carry `RpcBridge` dispatch tests with `MockVm` — but
those pin contracts, not the feature. The e2e run is the proof.

#### Running the boot-control suite (~5 minutes)

Proves the launcher, serial-console control, 9p, SSH login, and graceful shutdown against a
real guest. Needs a cloud-init-capable qcow2 **matching the host architecture**:

```bash
TDDY_CLOUDINIT_BASE_IMAGE=/path/to/debian-12-genericcloud-<arch>.qcow2 \
  ./dev cargo test -p tddy-vm --test vm_boot_control_acceptance -- --ignored --test-threads=1
```

`--test-threads=1` is not optional: the tests use fixed host ports (2231–2236), and
`#[serial]` only orders tests within one binary. If a run is interrupted, clear any orphans
before the next one — a leaked QEMU still holding a forwarded port makes the next run fail
for an unrelated reason:

```bash
pkill -f qemu-system-; rm -f /tmp/tddy-vm-monitor-*.sock
```

#### Running the bake (hours)

The full `BuildTddyHostImage` path. Budget several hours: it installs a 9p-capable kernel,
installs Nix, and runs a cold `./release` for the whole workspace — including `libwebrtc` —
inside a 2-vCPU guest.

```bash
TDDY_CLOUDINIT_BASE_IMAGE=/path/to/debian-12-genericcloud-<arch>.qcow2 \
  ./dev cargo test -p tddy-vm --test tddy_host_vm_acceptance -- --ignored --nocapture \
  bakes_a_prepared_base_whose_guest_runs_the_tddy_daemon_under_systemd
```

`--nocapture` matters — it is the only way to watch the streamed serial console, which is
where a stalled `apt`, a failed Nix install, or a wedged `./release` becomes visible. The
guest's full transcript is also written to `<output-dir>/<name>-boot.log`.

On success the prepared base lands in `<tddy-data-dir>/images/02-prepared-base/` as a
single sealed-`0444` delta (`debian-12-tddy.qcow2`) chained onto the image in `01-base/`.
(Updated: 2026-08-14 — formerly a `-base.qcow2` + overlay pair.) The scratch
directory is removed on both the success and failure paths, so a failed run leaves no
plaintext seed behind — but it also leaves nothing to inspect, so capture the console output.

The two follow-on tests reuse that output instead of re-baking, and run in about a minute:

```bash
TDDY_TDDY_HOST_PREPARED_BASE=<tddy-data-dir>/images/02-prepared-base/debian-12-tddy.qcow2 \
  ./dev cargo test -p tddy-vm --test tddy_host_vm_acceptance -- --ignored --test-threads=1
```

**When the bake is first run, check that** the LiveKit `api_secret` does not appear in
`<name>-boot.log` or the streamed RPC progress. (The other open question — whether the
completion token could be emitted before `runcmd` had finished — was settled by a real
bake: it could, and it was. See the `scripts-per-boot` note above.)

`<name>-boot.log` is written with terminal escapes stripped, and carries the guest's own
cloud-init logs, framed for extraction:

```bash
sed -n '/TDDY_GUEST_LOG_BEGIN \/var\/log\/cloud-init.log/,/TDDY_GUEST_LOG_END \/var\/log\/cloud-init.log/p' \
  <name>-boot.log
```

### Out of scope for this sub-feature

- Any `tddy-web` change, including a UI for the new methods.
- Downloading a base image — the Debian cloud image stays caller-supplied.
- Cloning project repos into the VM: that is the existing `CreateProject` / `StartSession`
  flow against the guest daemon, unchanged.
- Host-side peer registration — the guest self-announces on the LiveKit common room.
- Buildroot-image lineage and the `tddy-sandbox-qemu` 9p guest work, which stay as they are.

### Guest kernel: virtio-9p needs the generic kernel flavour (Added: 2026-08-02)

Debian's *cloud* kernel — what every `genericcloud` image runs — is trimmed and ships **no 9p
modules at all**, so the source share cannot be mounted on a stock image:

```
$ uname -r
6.1.0-21-cloud-arm64
$ modprobe 9p
FATAL: Module 9p not found in directory /lib/modules/6.1.0-21-cloud-arm64
$ mount -t 9p ... /mnt/tddy-src
mount: unknown filesystem type '9p'      # exit 32
```

The host side was never at fault: QEMU attaches the `-fsdev` / `virtio-9p-pci` pair and boots
normally.

The recipe therefore begins by putting the guest on a 9p-capable kernel
([`tddy_host::ninep_capable_kernel_command`]): install `linux-image-<arch>` (the generic
flavour, which carries the 9p modules), **purge the cloud flavour**, `update-grub`, and
reboot. Installing alone is not sufficient — with both flavours present GRUB keeps booting
the cloud one, and this image has no `GRUB_FLAVOUR_ORDER` setting to redirect it. Removing
the cloud packages is what makes the generic kernel the only thing left to boot.

The step is guarded on `uname -r | grep -- '-cloud'`, so it is a no-op once a generic kernel
is running. That bounds it to exactly one reboot: cloud-init re-runs `runcmd` on the next
boot (the step reboots through `cloud_init::reset_cloud_init_and_reboot`, which discards this
instance's cloud-init state first — without that, the next boot recognises an instance it has
already provisioned and skips `runcmd` entirely), and the second pass falls straight
through to the real provisioning work. **That "next boot" only exists because the bake's boot
argv omits `-no-reboot`** (see "Bake-in by booting" above) — with it, the guest's reset would
end QEMU instead of restarting the guest, and the bake would fail on every `genericcloud`
image, which is the only input this feature supports.

**A failed kernel install must fail the bake** (Added: 2026-08-02). cloud-init concatenates
`runcmd` into a single shell script and adds no error handling, so this step captures the
`apt-get` chain's status and exits with it rather than with a hardcoded `0`; the script also
opens with `set -e`. Without both, a failed step (an unreachable mirror, a failed `rsync`)
skipped only the rest of its own `&&` chain, `runcmd` still exited 0, cloud-init recorded no
error, and the host sealed and promoted a prepared base containing no tddy at all — reported
as a successful bake.

Verified end-to-end on `debian-12-genericcloud-arm64`: the guest comes back on
`6.1.0-51-arm64`, the kernel logs `9p: Installing v9fs 9p2000 file system support`, and the
host's file is read from inside the guest.

**Cost:** roughly three minutes and a ~100 MB download added to the front of each bake, plus
one extra reboot. A caller who supplies a Debian *generic* (rather than *genericcloud*) base
image skips all of it, since the step's guard sees a non-cloud kernel and does nothing.

### Known gaps / pending design — daemon-spawned tddy host VM (Added: 2026-08-02)

- **The bake has never been run end to end.** Accepted deliberately when this shipped. Every
  prerequisite it stands on *is* verified against a real guest — arch/accel/UEFI boot, serial
  console login and command execution, the 9p share, SSH with the per-VM key, graceful
  shutdown — and the pure recipe-rendering is unit-tested. What remains unproven is the long
  tail inside the guest: the Nix install, `./release`, `bun run build`, and
  `./install --systemd` actually completing. Two criticals found during PR wrap
  (a `systemctl reboot` incompatible with `-no-reboot`, and a failed kernel install reporting
  success) were both on this path and both invisible to the boot-control suite, so treat the
  first real bake as likely to surface more. See "Running the bake" for how.
- **`packages/tddy-web/src/gen/vm_pb.ts` is not regenerated.** Accepted deliberately: this
  changeset is RPC-level and no web code consumes the new messages, so nothing is broken. But
  `vm.proto` and the TypeScript client are out of sync until someone runs
  `bunx buf generate ../tddy-service/proto` from `packages/tddy-web` on a machine where
  `bun install` completes — it wedged on the development host.

- **The LiveKit credential is baked into a shared, world-readable prepared base.** This is a
  known limitation, not an oversight. `build_tddy_host_image` writes the `common_room` `url`
  / `api_key` / `api_secret` into `/etc/tddy/daemon.yaml` *inside* the image, and prepared
  bases are deliberately sealed `chmod 0444` in `images/02-prepared-base/` so nothing mutates
  them. Two consequences follow, and both are accepted for now:
  - **Any local account on the host can read the secret** out of the qcow2. Inside the guest
    the file is at least `0640 root:tddy` (deferred `write_files`, so the chown happens after
    cloud-init creates the user) rather than the `0644` the other tddy configs use, since
    this is the first tddy config to carry a live secret — `daemon.yaml.production` ships its
    whole `livekit:` block commented out.
  - **Every VM cloned from the base shares one credential**, so rotating it means baking a
    new prepared base — hours, per "the bake is slow" above — instead of restarting a VM.

  The fix is to stop baking the secret and inject it **per VM** at
  `CreateVmFromPreparedBase` time via a small per-VM NoCloud seed: `VmConfig::seed_iso`
  already exists for exactly this and is currently unused. That is a design change and is
  deliberately out of scope here. The bake's own scratch directory — which holds the rendered
  `user-data`, the seed ISO, and the generated SSH private key — is created `0700`, has its
  `user-data` written `0600`, and is removed on both the success and the failure path, so the
  plaintext copy does not outlive the bake even though the baked-in copy does.
- **Two opposite policies for a failed YAML render.** `tddy_host::daemon_config_yaml`
  `expect`s (correct: the document is owned `String`/`&str`/`u16` only, so a failure is a
  programming error), while `cloud_init::render_user_data_doc` falls back to
  `unwrap_or_else(|e| format!("# failed …"))`, which would hand cloud-init an
  empty-but-valid `#cloud-config` and bake an unprovisioned image while reporting success.
  Left as-is in this pass; the fallback should become an error return.
- **The bake is slow.** `./release` builds the whole workspace including `libwebrtc`, on top
  of a Nix installation, inside a 2-vCPU guest — expect hours for a cold run. This is why the
  bake is a separate RPC whose output is reused, and why its acceptance test is
  manual-trigger only.
- **Guest disk sizing is a guess.** The prepared base is sized for a full Rust target
  directory plus a Nix store; the default may need raising after the first real bake.
- **A baked tddy host has no serial-console credential.** `TddyHostSpec` deliberately carries
  no password: the provisioned user authenticates with the per-VM SSH key, and putting a
  password in the recipe would place a credential in generated production config. The
  consequence is that a tddy host whose *networking* is broken cannot be logged into at all —
  sshd being independent of the daemon means SSH still covers the ordinary "daemon failed to
  start" case, but not a broken NIC. The `serial_shell` driver can log in given credentials,
  so closing this is a matter of deciding where a console credential should come from
  (`LoginPolicy`, an operator-supplied override, or a one-shot break-glass account).
- **No console log-level control.** The guest boots with the image's default kernel/systemd
  console verbosity (~713 serial lines in the first 17 seconds), all of which is streamed as
  RPC progress and written to the boot log. `cloud_init_boot_argv` emits no kernel cmdline at
  all today, so there is no `loglevel=`/`quiet` knob to turn down.

  There is no prior art to copy here: `~/Code/makers-lt` has no guest-side loglevel control
  either — no `printk`, `dmesg -n`, or `console=`/`loglevel=` kernel argument anywhere, and
  its `qemu-vm-builder` exposes no `-append`. It handles console noise purely host-side, by
  pattern-matching sentinel lines and gating echo behind a `debug` namespace. Turning guest
  verbosity down is therefore new design work, deferred until the bake's real
  signal-to-noise is known.

## Out of scope for this changeset

- ScreenShare VM mode.
- Multi-host VM management.

## Known gaps / pending design (Updated: 2026-06-21)

- **`BuildVmImage` backend is wrong for spec-based builds.** The current implementation passes the UI textarea content as a tddy-build target ID. The correct implementation must accept a Buildroot config spec as text and invoke Buildroot directly — completely independent of the tddy-build graph. Requires: (a) agree on spec format (defconfig name / full `.config` / fragment); (b) establish Buildroot install path on the daemon host; (c) new RPC field or new RPC method; (d) new `build_vm_image_from_spec` Rust function.
- **`ListVmImages` RPC does not exist.** Dropdown images currently only accumulate within a single browser session. A persistent image registry (list of previously built qcow2 paths) needs its own storage and RPC.

## Known gaps / pending design — QEMU sandbox backend (Added: 2026-07-01, updated 2026-07-02)

- **No uid/user field in `SandboxPlan`.** The guest runs as root, matching the existing `QemuVm` SSH-as-root behavior. Per-mount uid mapping (9p `security_model`) is future work if a non-root guest identity is needed.
- **Guest image must opt in to 9p, and the fragment is unverified in a real boot.** Existing Buildroot specs built via `BuildVmImage` do not enable `CONFIG_NET_9P`/`CONFIG_9P_FS`; a kernel Kconfig fragment and BusyBox init hook now exist (`packages/tddy-sandbox-qemu/guest/`, documented in [guest-image-9p-init.md](../../../packages/tddy-sandbox-qemu/docs/guest-image-9p-init.md)) and their shell logic was verified against simulated inputs on the host, but no image has actually been built with this fragment and booted yet — that's the next real-world validation step before this backend can run end-to-end.
- **Guest command exit code is an approximation.** The `SessionChannel` protocol carries a real exit code in `SessionEnded`, but `run_host_relay` (shared by 7 call sites across darwin/cgroups/the daemon/tests) doesn't currently surface it to callers — plumbing it through touches that shared, already-working infrastructure. `tddy-sandbox-qemu` reports `0` on a clean session end and `1` on any connect/boot/relay failure, not the guest command's real exit code.
