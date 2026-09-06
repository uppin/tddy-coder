# 2026-08-03 — tddy-supervisor — VM-backed acceptance test

**Category:** Future enhancement

> **Implemented 2026-08-14** by the `tddy-vm-testkit` changeset — see
> [docs/dev/1-WIP/vm-cgroups-testkit.md](../1-WIP/vm-cgroups-testkit.md) and
> [plans/vm-cgroups-testkit.md](../../../plans/vm-cgroups-testkit.md). Steps 1-6 below are all
> in place, with two deliberate departures: step 1's **download** was dropped (the base
> image is supplied on disk via `TDDY_CLOUDINIT_BASE_IMAGE`, nothing is ever fetched), and
> the single bake of step 2 became a **three-image chain** sharing one Nix-prepared parent,
> so the builder and the guest under test derive from the same base without paying for Nix
> twice. Step 6's gRPC assertions are the remaining gap: the cross-user session and
> `PR_SET_PDEATHSIG` properties still need a tonic client over `ssh -L`. Keep this section
> until that lands.

The supervisor's 33 acceptance tests run the real binary but declare the *invoking* user as the service
user, so `privilege_to_drop` returns `None` and no drop happens; the cgroup base is a temp directory.
Three properties therefore have no automated coverage anywhere, and they are the feature's headline
claims:

- a session for OS user `alice` actually running as `alice` while the daemon runs as `tddy`;
- real cgroup v2 delegation with **enforced** limits (`rmdir` of an emptied scope succeeding, a
  populated one returning `EBUSY` — a plain directory returns `ENOTEMPTY` forever, so the retry path
  and the success path only execute on cgroupfs);
- `PR_SET_PDEATHSIG` surviving a real privilege drop. This one hid a live bug once already:
  `commit_creds()` zeroes `pdeath_signal`, and the property held in tests only *because* no drop was
  planned.

Design settled during reconnaissance, so this is implementation rather than open design:

1. **Base image.** Fetch a public Debian *genericcloud* qcow2, verify its checksum, cache it via
   `VmLibrary::import_base_image` into `images/01-base/`. The download step is genuinely absent from
   `tddy-vm` by explicit design decision (`docs/ft/vm/tddy-vm.md` lists it as out of scope) — it is the
   first thing to write.
2. **Bake once.** cloud-init it into `images/02-prepared-base/` as a single delta chained onto
   `01-base/`, sealed `0444`. (Updated: 2026-08-14 — was a flattened-base + overlay pair promoted
   by `promote_prepared_base_pair`; both are gone.) Bake **OS packages and the account
   only** — do *not* reuse `build_tddy_host_image`, whose recipe mounts the repo over 9p and runs a
   cold `./release` including `libwebrtc` inside the guest (`TDDY_HOST_BAKE_TIMEOUT` is 6 hours, and
   its comment says hours is expected). Baking without 9p also avoids the ~3 min / ~100 MB kernel swap
   that Debian's *cloud* kernel forces, since it ships no 9p modules at all.
3. **Per boot.** A fresh qcow2 overlay off the prepared base via `library::vm_overlay_create_argv`
   (absolute backing), so base images are never mutated. Note `VmLibrary::create_vm` writes to a fixed
   `vm/<name>/<name>.qcow2` and `qemu-img create` fails if it exists, so a per-boot path scheme is
   needed.
4. **Reusable VM.** Mirror the LiveKit pattern exactly — a `run-tddy-vm-testkit` script plus an env var
   carrying the forwarded port, with the testkit skipping teardown when the var was supplied
   externally (`packages/tddy-livekit-testkit/src/livekit_testkit.rs` and
   `run-livekit-testkit-server`). Nothing analogous exists for VMs today: every VM acceptance test
   boots and shuts down its own guest.
5. **Provisioning.** `scp` the host-built `tddy-supervisor`/`tddy-daemon`/`tddy-tools` over the
   always-present `tcp::<port>-:22` forward and run `./install --systemd`. Re-testing a code change is
   then an scp, not a re-bake.
6. **Assertions over gRPC.** The daemon's local socket speaks tonic gRPC and `tddy-service` already
   generates `ConnectionServiceClient`, so `ssh -L <port>:/run/tddy-daemon.sock` plus a tonic `Channel`
   needs no new client code — unlike the Connect surface on the web port, for which the repo has no
   Rust client at all. This also reaches the daemon *through the socket the supervisor creates and hands
   over as fd 3*, giving that handoff end-to-end coverage it cannot get in CI. `tddy-tools --mcp`
   (configured by `TDDY_REMOTE_DAEMON_URL`/`TDDY_REMOTE_SESSION_ID`, not a `--proxy` flag) is the
   guest-side tool path.

Two traps found while surveying, worth carrying:

- **`livekit.api_secret` *is* the session-token HMAC secret.** Setting `livekit: None` is not the clean
  escape it looks like: with no secret the guest daemon returns `Unauthenticated` for every RPC with no
  fallback. A secret must be configured even if LiveKit is never used — and since the harness chooses
  it, the host can mint its own access tokens with `SessionTokenSigner`, or use the `github: { stub:
  true }` provider.
- **`daemon_config_yaml` in `tddy_host.rs` emits no `github:`, `users:` or `supervisor:` block**, so
  guest config emission has to be extended or written directly.

Live where it creates no cycle: `packages/tddy-e2e/tests/` (it already holds `install_supervisor.rs`;
`tddy-vm` depends on neither the daemon nor the supervisor). **Not** `packages/tddy-supervisor/tests/`,
which would need `tddy-daemon` for the client side and that is a cycle. Follow the existing production
test conventions — `#[ignore]` + `#[serial]` + env-gate + early return — so `./test` stays unaffected.

Prerequisite on any machine that runs it: `/dev/kvm` must be *openable*, not merely present. Under TCG
the bake takes hours. `VmAccel::host_default` now tests openability rather than existence, so a host
without access correctly reports `Tcg` instead of producing a manifest QEMU refuses to start.
