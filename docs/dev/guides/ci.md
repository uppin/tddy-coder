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
| VM-backed tests (`./vm-tests`) | `#[ignore]`d by design; need a QEMU guest and a base image that is never downloaded |
| cgroups sandbox tests | Need root and a writable cgroup root; the backend reports itself available on any Linux, then EPERMs at spawn |
| One `tddy-sandbox-recipes` path test | Asserts a macOS-only path layout |
| `sandbox_runner_stdio_acceptance::echoes_a_message_over_sandbox_service_served_over_stdio` | Fails unprivileged with "tool ipc server exited before bind" even with the runner binary staged; survived two retries. Only this one test is skipped — the other two in the binary pass. Tracked in `docs/dev/TODO.md`; the intended fix is a VM-backed job, not a permanent exclusion |
| Cypress **e2e** specs | Storybook build plus a real ghostty WebGL context; not yet verified on a runner |
| `tddy-desktop`, `tddy-rust-typescript-tests` | Need Electron and cross-language fixtures respectively |

The Rust exclusions live in `.config/nextest.toml` under `[profile.ci]`
`default-filter`, each with its reason. Keep that list short — every entry is a
hole in the gate. Removing one is the preferred fix; suppressing a newly failing
test there is not.

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

CI reports status but does not block merges until branch protection says so. In
**Settings → Branches → `master`**, require these status checks:

- `Rust lint`
- `Rust tests`
- `Rust build`
- `Web tests`

## Forks

`GITHUB_TOKEN` is read-only for PRs from forks, so the check runs that carry test
counts cannot be created there. The jobs still run and still pass or fail; only
the annotated test report is missing. If external contributions become common,
move the reporting step into a `workflow_run` follow-up workflow.
