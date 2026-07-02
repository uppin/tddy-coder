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

- **Immutable base + chained delta overlay.** The base cloud image is **copied** from
  the caller-provided source (never downloaded by this feature, never re-fetched, never
  mutated) into `<output-dir>/<name>-base.qcow2` via `qemu-img convert -f qcow2 -O
  qcow2`. A delta overlay `<output-dir>/<name>.qcow2` is created with `qemu-img create -f
  qcow2 -F qcow2 -b <name>-base.qcow2 <overlay> <disk-size>` — a **relative** backing
  reference, so the base and overlay must stay co-located in the same directory (the
  "image" produced by this feature is the pair, not a single file). Mirrors the existing
  ephemeral-overlay primitive in `tddy-sandbox-qemu`'s `overlay_create_argv`
  (`packages/tddy-sandbox-qemu/src/argv.rs`), adapted for disk sizing and relative
  basenames.
- **NoCloud cloud-init seed.** User-data/meta-data are rendered to a `seed/nocloud/`
  directory and packed into a `cidata`-labeled ISO9660 image (Joliet + Rock Ridge) via
  `xorriso -as mkisofs` (mkisofs-emulation mode — no new Rust ISO dependency). The
  `{{SSH_PUBLIC_KEY}}` placeholder in `ssh_authorized_keys` is replaced with either a
  caller-supplied key (`--ssh-public-key`) or a freshly generated keypair
  (`ssh-keygen`). A `cloud-init clean --logs --seed` `bootcmd` is injected so a
  pre-baked cloud image's prior cloud-init state doesn't suppress re-provisioning on the
  copy.
- **Bake-in by booting.** The overlay is booted with the seed ISO attached
  (`-cdrom`), reusing `QemuVmArgs`' argv shape (`packages/tddy-vm/src/qemu.rs`) with the
  differences needed to observe completion: `-serial stdio` (not `file:`) and
  `-no-reboot`. A deterministic completion token
  (`CLOUDINIT_COMPLETE_<name>_<sha256(provisioning-input)[:12]>`) is embedded in
  user-data as a per-boot script that prints the token then calls `shutdown -h now` (or
  `<token>_FAILED` on error); the host watches the serial stream line-by-line (reusing
  the `BufReader`/`tokio::select!` draining pattern from `build.rs::run_parallel_build`)
  and returns once the token is observed, with `send_monitor_command`
  (`packages/tddy-vm/src/qemu.rs`) as a graceful-shutdown fallback on timeout. The
  overlay this produces is fully provisioned — no first-boot cloud-init step needed by
  the consumer.
- **New module `tddy_vm::cloud_init`** — all argv/document-rendering logic is exposed
  as pure, unit-testable builder functions (`base_convert_argv`, `overlay_create_argv`,
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
backing files (pristine download → flattened immutable base → cloud-init overlay → per-VM
mutable overlay), reusing this crate's existing `cloud_init` argv builders
(`base_convert_argv`, `overlay_create_argv`) rather than reinventing them.

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
  cloud-init build's outputs into the library: the downloaded input base into `01-base/`,
  and the flattened base + provisioned overlay pair into `02-prepared-base/` (co-located,
  preserving the relative-backing-file invariant the pair depends on). `tddy-vm-build
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
