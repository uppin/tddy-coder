# 2026-08-14 — VM image chaining and testkit — deliberate gaps

**Category:** Future enhancement
**Source:** vm-cgroups-testkit changeset, 2026-08-14

- **BLOCKER: binaries built in the builder guest cannot execute on the test host.** They are
  compiled inside the Nix dev shell, so their ELF interpreter is an exact store path —
  `/nix/store/nmq81hidzwij3c7vyiazwg2l74vnxkar-glibc-2.42-51/lib/ld-linux-aarch64.so.1`. The test
  host inherits `/nix` from `tddy-nix-base` but not *that* glibc closure, which was only ever
  realized in the builder's dev shell, so `execve` fails `ENOENT` on the interpreter and systemd
  reports `203/EXEC`. Observed as `./install --systemd` succeeding completely and then
  `tddy-supervisor is 'activating', not active`. Four ways out, none free: build against **musl**
  statically (cleanest "deployable" story, but a real change to how the workspace builds and
  `libwebrtc` is the risk); **`nix copy`** the runtime closure to the test host (faithful to how a
  Nix artifact really deploys, but the test host stops resembling a plain production host); build
  with **Debian's own toolchain** in the guest (links against system glibc, but the builder stops
  exercising the real `./release` path); or **warm the dev shell on the test host** (smallest
  change, deliberately reintroduces a toolchain into the guest the tests treat as production-like).
  This blocks all five cgroups e2e tests.
- **`builds_deployable_linux_binaries_on_a_host_that_cannot_compile_them` asserts too little.** It
  checks the ELF header's `e_machine` and stops, so it passed while producing binaries that cannot
  actually run on the guest they are built for — "deployable" is exactly the property it does not
  test. It should assert the binary *executes* in the test host (e.g. `--version`), which is the
  only check the interpreter problem could not have survived.
- **AppArmor profile does not load on Debian 12** (non-fatal): `apparmor_parser` fails with
  `Could not open 'abi/4.0'` — the profile targets a newer abi than bookworm ships. `./install`
  warns and continues, but on a host with `kernel.apparmor_restrict_unprivileged_userns=1` the
  daemon's own sandbox jails would fail to create a user namespace.

- **`tddy-vm-build cloud-init` can only build the *first* layer of a chain** (found by running it,
  2026-08-14). `run_cloud_init_build` unconditionally calls `import_base_image`, which now rejects a
  qcow2 that names a backing file — correctly, since importing a delta into `01-base/` would strand
  it. So passing an already-prepared layer as `--base-image` fails with *"is a qcow2 delta with a
  backing file; import the whole image it ultimately derives from instead"*. Multi-level chaining
  works only through `tddy-vm-testkit`'s `bake.rs`, which hands the parent straight to
  `build_cloud_init_image` without importing. Closing it means letting the CLI distinguish "import
  this pristine image, then chain onto it" from "chain onto this existing layer" — probably a
  `--parent-layer` flag alongside `--base-image`, mutually exclusive.

- **`VmManager::start` still boots with `seed_iso: None`** (`packages/tddy-vm/src/registry.rs:286`).
  `create_vm` now writes a per-VM NoCloud seed authorizing the keypair it generates, and both the
  testkit and `packages/tddy-vm/tests/common/mod.rs` attach it — but a library VM started **through
  the daemon** does not, so SSH into it cannot authenticate (`BatchMode=yes` + `IdentitiesOnly=yes`
  leave no fallback). Closing it means distinguishing library-created VMs, which have a seed, from
  spec-only VMs pointed at an arbitrary `image_path`, which have none and would fail to boot on a
  missing `-cdrom`. That is an RPC-surface decision, not a one-liner.
- **No layer records its parent's identity.** `import_base_image` unconditionally removes and
  re-copies `images/01-base/<name>.qcow2` on every bake, and qcow2 stores no parent hash — so a
  re-imported base silently changes the bytes under every existing child and nothing detects it.
  This is the makers-lt gap the changeset set out to close by recording each layer's parent in the
  manifest; it is not implemented and no test covers it.
- **The five VM production tests have never been run end to end.** Their gating is verified (all
  report `ignored` in a default run) but the bake chain itself is unexercised — the first real run
  should expect corrections around the guest-side `./install` invocation and the `tddy.slice` path
  the delegation assertions read.
- **A non-qcow2 supplied base image is not normalised.** `import_base_image` rejects a *chained*
  qcow2 but copies a raw/VMDK source verbatim, which then fails later at `qemu-img create -F qcow2`
  with a confusing error. If normalisation is added, its argv cannot live in `library.rs`: the
  `no_disk_flattening_acceptance` guard matches per-file on `"convert"` + `"-f"` + `"qcow2"`, and
  `library.rs` already contains `"-f"` from `ssh-keygen`.
- **The anti-flattening guard matches source text**, so it passes for a renamed reintroduction of
  the same behaviour and can false-positive on an unrelated `"convert"` literal. A tripwire, not a
  proof.
- **`dist/linux-aarch64` is hardcoded** (`packages/tddy-vm-testkit/src/layout.rs`) while
  `VmArch::host()` can be x86_64, and `vm_cgroups_acceptance.rs` asserts `EM_AARCH64`
  unconditionally. No arch guard.
- **~150 lines duplicated** between `packages/tddy-vm-testkit/src/guest.rs` and
  `packages/tddy-vm/tests/common/mod.rs` (`force_kill` and `wait_for_port_release` are byte-identical;
  `boot_library_vm` is `BootedGuest::boot` with no shares), already drifting in their env-var
  constants. `tddy-vm` could take `tddy-vm-testkit` as a dev-dependency and keep only
  `TestGuestBuilder`, which has no testkit equivalent.
- **`tddy-vm-testkit` is a plain workspace lib**, so nothing structurally stops production code
  depending on it and picking up `SESSION_TOKEN_SECRET` / `GUEST_PASSWORD`. Consider
  `publish = false` plus a crate-level test-only marker.
