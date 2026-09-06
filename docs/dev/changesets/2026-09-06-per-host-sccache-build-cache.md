# 2026-09-06 — Per-host sccache build cache wired through `./dev`

Rust compilation is the bulk of the time in this workspace, and `cargo` discards
all of it whenever `target/` is wiped, a branch swaps a dependency, or a worktree
is created. sccache caches compilation units by their inputs instead, so the work
is done once per input rather than once per `target/`.

## The delta

Where those units live is a property of the **host**, so the repo ships the
wiring and the host names the backend.

- **`scripts/build-cache-env.sh`** (new) — resolves the host's answer and prints
  it as `export` lines. Precedence: `TDDY_BUILD_CACHE` (names a backend
  directly, YAML unread) → `~/.tddy/build-cache.yaml` (path overridable with
  `TDDY_BUILD_CACHE_CONFIG`) → nothing, and sccache stays off.
- **`./dev`** and **`.envrc`** eval it. `./dev` covers `./test`, `./verify`,
  `./release`, `./vm-tests`, `./web-dev`, `./install`, `./publish.sh` and
  `./run-vm-testkit`, all of which shell out to it; `.envrc` covers the direnv
  path, quietly, since it runs on every `cd`. Neither holds a copy of the logic.
- **`flake.nix`** — `pkgs.sccache` (0.14.0), so whether it is *used* is the only
  variable.
- **`.github/workflows/ci.yml`** — **unchanged**. CI leaves sccache off; see
  "What CI measured" below for why, and the guide for the exact wiring to turn
  it back on.
- **`build-cache.example.yaml`**, **`docs/dev/guides/build-cache.md`** —
  the copy-and-edit file and the guide.
- **`packages/tddy-e2e`** — `build_cache_contract` plus 19 tests in
  `tests/build_cache_env.rs`, which run the real script against configs in a
  temp `HOME` and assert on the environment a caller would inherit.

## Decisions worth keeping

**No implicit local cache.** A missing `~/.tddy/build-cache.yaml` leaves sccache
off and prints one notice. A cache means gigabytes on someone's disk and a new
way for a build to be wrong; opting in is the host owner's call, and a machine
that never opts in behaves exactly as it does today.

**A selected-but-unreachable backend fails the run** rather than falling back to
compiling uncached. An sccache that cannot reach its cache still compiles
everything, so the only symptom of a silent fallback would be a build that is
quietly slow forever.

**`type:` selector plus one block per backend**, rather than flat keys. A
configured Redis endpoint can sit unused while you work off local disk, so
switching backends is a one-line edit instead of a rewrite.

**`CARGO_INCREMENTAL=0` whenever a backend is enabled.** Not a preference:
sccache declines any unit carrying `-C incremental`, so cargo's dev-profile
default would keep the cache permanently empty.

**`vm-tests.yml` left alone** — its expensive compile happens inside a QEMU
guest, which cannot reach the runner's cache service.

## Measured

`tddy-core` and its dependency tree on an M-series laptop, `local` backend:
cold **62s** (278 units cached, 0 non-cacheable), then **26s** from an emptied
`target/` at a **100% hit rate**.

## What CI measured, and why CI uses Redis

The `github-actions` backend was wired up, tested over five CI runs, and then
deliberately removed. It works — `Rust build (arm64)` went from 4m02s cold to
**2m56s at a 100% hit rate** — but it does not fit this repo's cache budget.

GitHub caps a repository's Actions cache at **10 GB**, and sccache stores one
entry per compilation unit. Two runs wrote **1960 entries (1.5 GB)**. The cache
was already over budget beforehand — 11 entries totalling 15.3 GB, which is what
the `save-if: master` rules were meant to prevent — and the extra pressure
evicted the 2.6 GB `v0-rust-build-Linux-x64` archive.

That is a losing trade. `Swatinem/rust-cache`'s `target/` archive is what makes
these jobs finish in four minutes; displacing it means more units compile, which
makes sccache write more entries, which evicts more. **Redis keeps the build
cache off that budget entirely**, which is what CI ships. The `github-actions`
backend stays supported and documented for a repo with cache headroom.

Three findings from that experiment are worth keeping, because each cost a CI
round trip and none is visible from the outside:

- **`ACTIONS_CACHE_SERVICE_V2` selects the cache protocol.** Without it sccache
  builds v1 paths (`_apis/artifactcache/…`) and sends them to the v2 host; since
  GitHub decommissioned v1, every read and write 404s. The only symptom is a
  `Cache write errors` counter and a cache that never fills — from the outside it
  is indistinguishable from a cold cache, and two consecutive runs both reported
  0% before the log was turned on.
- **A job-level `env:` has no `runner` context.** Naming one is not a bad value
  but an invalid workflow file: GitHub starts no jobs and the check is red with
  nothing in it. `no_job_level_env_reaches_for_the_runner_context` now guards
  every workflow in the repo.
- **Cache entries are scoped by ref.** A `workflow_dispatch` run
  (`refs/heads/<branch>`) and a `pull_request` run (`refs/pull/N/merge`) are
  sibling scopes that cannot read each other, so a hit test has to use the same
  event twice or it measures nothing.
