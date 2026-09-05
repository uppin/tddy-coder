# Continuous Integration

CI runs on **GitHub Actions** (`.github/workflows/ci.yml`). The repo is public, so
Actions is free with unlimited minutes on 4-vCPU / 16 GB Linux runners.

Every pull request gets four checks. The test checks publish a **test count**,
not just a colour, so a red check names the tests that failed.

| Check | What it runs | Cold-cache time |
|-------|--------------|-----------------|
| `Rust lint` | `cargo fmt --all --check`, then `cargo clippy --workspace --all-targets -- -D warnings` | ~8 min |
| `Rust build` | `cargo build --workspace --bins --examples` | ~10 min |
| `Rust tests` | `cargo nextest run --workspace --profile ci` | ~15 min |
| `Web tests` | `bun install --frozen-lockfile`, `bun run build`, `tddy-web` unit tests, `tddy-web` + `tddy-livekit-web` Cypress component tests | ~10 min |

`Rust lint` runs on its own. **`Rust build` runs first, and both test checks
wait on it**, because a number of suites exec a workspace binary by path rather
than linking it, and cargo never builds those as a side effect of running the
tests — they belong to a different package:

| Binary | Exec'd by |
|--------|-----------|
| `target/debug/tddy-sandbox-runner` | `tddy-daemon` `sandbox_runner_stdio_acceptance` |
| `target/debug/tddy-acp-stub` | `tddy-integration-tests` `acp_*`, `codex_acp_*` |
| `target/debug/examples/echo_server` | `tddy-livekit-web` Cypress specs |

Jobs get separate runners, so the build publishes those three as the
`rust-fixture-bins` artifact and each test job downloads them into
`target/debug`. They are stripped first (execution fixtures, nobody reads their
symbols) and the executable bit is re-applied on download, since artifacts are
zipped without permissions.

If you add a test that shells out to a workspace binary, add that binary to the
artifact — otherwise it will pass locally and fail in CI with "not built".

Worth being explicit about the limit of this: because the runners are separate,
building first shares the *binaries*, not the compilation. `Rust tests` still
compiles its own test targets from the cache. The gain is coverage of the suites
that need fixtures, not speed — it costs roughly the build's wall-clock time.

All three run in the **nix dev shell** via `./dev`, against the pinned
`flake.lock`. CI therefore uses the same rustc, clippy, bun and system libraries
you get locally — no second dependency list to keep in sync, and no class of
failure that only reproduces on a runner.

## Reading results

### From a terminal or an agent

```bash
scripts/ci-status.sh              # current branch's PR: checks + pass/fail counts
scripts/ci-status.sh --failures   # + failing test names, files, assertion messages, log tails
scripts/ci-status.sh --watch      # block until the run finishes, then report
scripts/ci-status.sh 396          # a specific PR
```

The counts and the failing test names come from check runs published by
`mikepenz/action-junit-report`, so they are available over the API — no artifact
download, no log scraping. The underlying calls, if you want them directly:

```bash
gh pr checks                                     # per-check status
gh api repos/{owner}/{repo}/commits/$SHA/check-runs \
  --jq '.check_runs[] | "\(.name): \(.output.title)"'   # "N tests run, M passed, K failed"
gh api repos/{owner}/{repo}/check-runs/$ID/annotations  # one annotation per failed test
gh run view $RUN_ID --log-failed                 # raw log of failing steps only
```

Raw JUnit XML is also uploaded as the `junit-rust` and `junit-web` artifacts
(14-day retention) for anything the API does not cover:

```bash
gh run download $RUN_ID --name junit-rust
```

## What the gate does not cover

The gate is scoped so that **red always means a regression**. These are excluded
deliberately, and each exclusion is coverage you still have to get locally:

| Excluded | Why |
|----------|-----|
| VM-backed tests (`./vm-tests`) | `#[ignore]`d by design; need a QEMU guest and a base image that is never downloaded. Two of them run in the separate VM workflow below |
| cgroups sandbox tests | Need root and a writable cgroup root; the backend reports itself available on any Linux, then EPERMs at spawn |
| One `tddy-sandbox-recipes` path test | Asserts a macOS-only path layout |
| `sandbox_runner_stdio_acceptance::echoes_a_message_over_sandbox_service_served_over_stdio` | Fails unprivileged with "tool ipc server exited before bind" even with the runner binary staged; survived two retries. Only this one test is skipped — the other two in the binary pass. Tracked in `docs/dev/TODO.md`; the intended fix is a VM-backed job, not a permanent exclusion |
| Cypress **e2e** specs | Storybook build plus a real ghostty WebGL context; not yet verified on a runner |
| `tddy-desktop`, `tddy-rust-typescript-tests` | Need Electron and cross-language fixtures respectively |

The Rust exclusions live in `.config/nextest.toml` under `[profile.ci]`
`default-filter`, each with its reason. Keep that list short — every entry is a
hole in the gate. Removing one is the preferred fix; suppressing a newly failing
test there is not.

## VM tests (separate workflow)

`.github/workflows/vm-tests.yml` runs the QEMU-backed production tests. It is a
separate workflow, not part of `ci.yml`, and **not a required check** — it is
slower, needs hardware virtualisation, and is new enough to want its own blast
radius.

It runs on pull requests **and on pushes to `master`**. The master half is not
redundant: `master` is the only ref that writes the cargo cache, so without it
the `vm` cache is never populated and every PR compiles `tddy-vm` from scratch.
It also means the branch itself has VM coverage rather than only its PRs.

Scope is what a bare cloud image can reach, which is two checks out of the nine
suites in `./vm-tests`. Both are legs of one matrix job, so they share the KVM
handling and the false-green guard:

| Check | Runs | Proves |
|-------|------|--------|
| `VM boot control` | all of `vm_boot_control_acceptance` (6 tests) | The launcher, serial-console control, 9p, SSH login with the per-VM key, graceful shutdown |
| `Cloudinit VM boot` | one test of `cloud_init_acceptance`, `a_baked_prepared_base_boots_as_a_vm_that_answers_over_ssh` | That a bake produces a *usable* layer: cloud-init bakes a prepared base, a VM is created from it, boots, and answers `id -un` over SSH as the account the bake provisioned |

Neither needs a pre-baked image chain, which is why these two and not the other
seven. CI downloads Debian 12 genericcloud amd64 (331 MB, checksum-verified
against the `SHA512SUMS` published beside it, then cached) because the testkit
never downloads anything itself by design.

`Cloudinit VM boot` runs one test rather than its whole binary deliberately. The
other two in `cloud_init_acceptance` each pay for their own bake and then only
*inspect* the result — qcow2 magic bytes, a relative backing reference — which
this one subsumes by booting it. `--exact` selects it by name, so adding a test
to that file does not silently add a bake to CI.

The runner is x86_64, and that matters in two ways. `VmArch::host()` reads
`std::env::consts::ARCH`, so the guest image must be **amd64**, not the arm64
one a developer on Apple silicon would use. And on x86_64 the `q35` machine
boots through SeaBIOS, so `uefi_firmware_for` returns `None` and no `edk2` image
has to be located — which is fortunate, since the nix QEMU package ships
`edk2-aarch64-code.fd` but no x86_64 equivalent.

### Two traps this workflow guards against

**KVM is optional to the code and mandatory in practice.** `VmAccel::host_default()`
falls back to `Tcg` when `/dev/kvm` cannot be *opened* — and `/dev/kvm` on a
GitHub runner is `root:kvm 0660`, visible but not openable. That fallback is
correct but roughly an order of magnitude slower, which would blow the suite's
180-second boot budget. The workflow installs a udev rule making the node `0666`
and then **fails fast** if a read-write open still does not succeed, so a runner
without virtualisation reports that in seconds instead of timing out at 90
minutes.

**A missing image used to pass.** Every one of these tests opened with
`let Some(base_image) = configured_base_image() else { eprintln!(...); return; }`,
so an unset variable or a failed download yielded a green check that booted
nothing — and an unset variable is exactly what a typo or a failed download
produces, which is when a green result misleads most. They now call
`require_base_image()` (`tddy-vm-testkit/src/env_file.rs`), which panics naming
the variable, so an unconfigured prerequisite is a red test.

Reporting a *deliberate* absence stays where it belongs: `./vm-tests` checks the
variable once, up front, and says so in one line rather than each test deciding
for itself.

The workflow still asserts the path exists before running, and afterwards greps
for `test result: ok. <n> passed` — `n` being the leg's own `expected_passing`, 6
or 1 — plus the old skip wording, kept as a regression guard against the pattern
coming back. A false green is worse than a red one.

### What it would take to run the rest

| Suite | Blocker |
|-------|---------|
| `tddy_host_vm_acceptance` (the bake) | Several hours even accelerated: installs a 9p kernel, installs Nix, and runs a cold `./release` of the whole workspace including `libwebrtc` inside a 2-vCPU guest. Per `docs/ft/vm/tddy-vm.md`, it has never been run end to end. Its output is a multi-GB qcow2 chain that does not fit the 10 GB Actions cache, so it would need external blob storage (ghcr.io via ORAS, or Releases) |
| `vm_cgroups_acceptance`, and the follow-on `tddy_host_vm_acceptance` tests | Consume a baked prepared-base, so they inherit the bake's problem |
| `cloud_init_acceptance` (the two tests CI does not run) | Nothing structural — each bakes from the bare cloud image like the one that does run. They are left out because a second and third bake buys inspection of an artifact the `Cloudinit VM boot` check already boots |
| `vm_library_acceptance`, the two `tddy-vm-build` CLI suites | Not yet triaged for whether they transitively need a baked base |

## Flaky tests

The LiveKit testkit picks a free port by binding `:0` and releasing it, leaving a
TOCTOU window before Docker claims it
(`packages/tddy-livekit-testkit/src/livekit_testkit.rs:26`). Under parallel load
another test can take the port first and the container fails to bind.

Two mitigations, both in `.config/nextest.toml`:

- Docker-backed tests run in a `docker` test group capped at one thread, which
  narrows the window.
- The `ci` profile retries up to twice with exponential backoff. Retried tests
  are reported as **flaky**, not silently passed, so the signal survives.

## Caching and the 10 GB budget

GitHub gives the repo 10 GB of Actions cache, evicted LRU. Three things compete
for it, so:

- The nix store is cached by `nix-community/cache-nix-action`, keyed on
  `flake.nix` + `flake.lock`, capped at 5 GB. **`Rust build` is the only job
  that writes it**; the other three restore with `save: false`. All four jobs
  need the identical devShell closure, so letting several of them save the same
  key just means they race for it — and the loser does not fail, it does the
  whole job first (garbage-collect the store, tar all of `/nix`) and only then
  discovers the key is taken. One writer avoids that work entirely, since the
  action's garbage collection only runs on the save path.
- `purge` is deliberately unset. GitHub already evicts caches LRU past the
  budget and drops entries untouched for 7 days, so purging is a racier version
  of what the platform does for free.
- Cargo artifacts are cached by `Swatinem/rust-cache` under three separate keys
  — `lint`, `test`, `build`. They cannot share one: clippy, `cargo test` and
  `cargo build` produce different artifacts for the same crate, so a shared key
  would have each job evicting the others' work. Three keys is the main pressure
  on the 10 GB budget; if caches start thrashing, dropping `cache-targets` on
  the lint job is the first lever.
- **Only pushes to `master` write caches.** PRs read master's copy. Otherwise
  every branch evicts every other branch and nothing stays warm.

A PR that changes `Cargo.lock` or `flake.lock` therefore pays a cold build once,
and master repopulates the cache on merge.

Runners guarantee only ~14 GB of disk, so each Rust job deletes the preinstalled
dotnet/android/ghc toolchains first, and debuginfo is reduced to
`line-tables-only` — enough for `file:line` in backtraces, a fraction of the size.

## Making the checks required

CI reports status but does not block merges until a ruleset says so. The
`master` ruleset requires these four contexts:

- `Rust lint`
- `Rust tests`
- `Rust build`
- `Web tests`

Do **not** add `VM boot control` or `Cloudinit VM boot` to that list yet — they
are still being proven out, and a required check that flakes on QEMU would block
every merge.

Creating it needs **repository admin**, so it lives here as a command rather
than as a file the repo can apply itself:

```bash
gh api -X POST repos/uppin/tddy-coder/rulesets --input - <<'JSON'
{
  "name": "master",
  "target": "branch",
  "enforcement": "active",
  "conditions": { "ref_name": { "include": ["~DEFAULT_BRANCH"], "exclude": [] } },
  "rules": [
    {
      "type": "required_status_checks",
      "parameters": {
        "strict_required_status_checks_policy": false,
        "do_not_enforce_on_create": false,
        "required_status_checks": [
          { "context": "Rust lint" },
          { "context": "Rust build" },
          { "context": "Rust tests" },
          { "context": "Web tests" }
        ]
      }
    }
  ]
}
JSON
```

`strict_required_status_checks_policy` is deliberately **false**. Strict mode
demands every PR be up to date with `master` before it merges, which on a
15-minute test job means a queue of two PRs re-runs CI on the second one every
time the first lands.

Note that the ruleset applies to **direct pushes to `master`** as well, not just
to PRs: a pushed commit has no check runs yet, so the push is rejected and every
change has to arrive through a PR. That is the intended state.

Resist adding a bypass actor for the **admin role** as an escape hatch. Beyond
reopening the hole, it can break `#automerge` outright: if the PR author's own
role may bypass the required checks, GitHub can consider that PR already clean
and refuse to arm auto-merge on it. The one bypass actor the repo does grant is
the GitHub Actions app, which is not a PR author — see the next section.

## Automerge

Three comment triggers, handled by `.github/workflows/automerge.yml`:

| Comment | Effect | Reaction |
|---------|--------|----------|
| `#automerge` | Merge — squashed — as soon as the four required checks pass | 🚀 |
| `#automerge-cancel` | Disarm it again | 👀 |
| `#forcemerge` | Merge **now**, past red or still-running checks | 🎉 |

A request that fails gets 😕. Only commenters with `write`, `maintain` or
`admin` are obeyed; the workflow asks the API for the actual permission level
rather than trusting the comment's `author_association`, which reports `MEMBER`
for any org member regardless of whether they can touch this repo. An
unauthorised comment gets **no** reaction at all — the run fails silently rather
than confirming to a stranger that the trigger exists.

`#forcemerge` is held to the same `write` bar rather than a higher one on
purpose. The repo is owned by a different account than the people working in it,
so an admin-only force merge would be usable by nobody. It leaves a trace that
outlives the run log: before merging, the workflow comments on the PR naming who
asked, linking their comment, and quoting the full check state at that moment.

### What it depends on

- **`Allow auto-merge` on the repo.**
  `gh api -X PATCH repos/uppin/tddy-coder -F allow_auto_merge=true` (admin only).
- **Required status checks on `master`** — the ruleset above. Native auto-merge
  is a queue for a *blocked* PR, not a merge button: with nothing required, a PR
  is mergeable the moment it opens and the API rejects the request with
  *"Pull request is in clean status"*. The ruleset is the mechanism, not
  decoration.
- **The GitHub Actions app as a bypass actor**, for `#forcemerge` only.
  `GITHUB_TOKEN` cannot skip a required check by default — a force merge fails
  at `gh pr merge --admin` rather than merging quietly. Granting it:

  ```bash
  gh api -X PUT repos/uppin/tddy-coder/rulesets/21793758 --input - <<'JSON'
  { "bypass_actors": [
      { "actor_id": 15368, "actor_type": "Integration", "bypass_mode": "always" }
  ] }
  JSON
  ```

  The cost is real and worth stating: *every* workflow in the repo then holds a
  token that can bypass required checks on `master`, `ci.yml` included. What
  makes that acceptable is that a workflow can only be changed through a PR.

### Two traps in the implementation

**`issue_comment` workflows always run from the default branch.** Editing this
workflow on a branch changes nothing until it is on `master`; the version that
runs for a PR's comments is master's, not the PR's. That is also what makes the
trigger safe on a public repo — a fork cannot alter what it executes. The
corollary is that the PR introducing a change to it cannot test that change, and
has to be merged by hand.

**`#automerge` asks the checks API, not `mergeStateStatus`.** It has to decide
between merging now and queueing, because queueing a PR with nothing left to
wait for is an error rather than a no-op. The obvious source for that decision is
`mergeStateStatus` — and it is the wrong one, because that field answers "is this
blocked *for you*". Once the Actions app is a bypass actor, the answer for
`GITHUB_TOKEN` stops being the answer for the ruleset, and `#automerge` would
quietly become `#forcemerge`. So the workflow runs `gh pr checks --required`,
which reports the checks themselves: exit 0 when every required one has passed,
non-zero while any is pending or failing. It correctly reads requirements from
the ruleset and excludes the VM checks.

The merge itself uses the repo's squash defaults (`COMMIT_OR_PR_TITLE` /
`COMMIT_MESSAGES`), which is what produces the `... (#406)` subjects in
`git log`.

## Forks

`GITHUB_TOKEN` is read-only for PRs from forks, so the check runs that carry test
counts cannot be created there. The jobs still run and still pass or fail; only
the annotated test report is missing. If external contributions become common,
move the reporting step into a `workflow_run` follow-up workflow.
