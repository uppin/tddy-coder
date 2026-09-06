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
- **`.github/workflows/ci.yml`** — `TDDY_BUILD_CACHE: github-actions` on all four
  Rust jobs, a `github-script` step to re-export the cache service's credentials
  (they are not in a `run:` step's environment), `SCCACHE_GHA_VERSION:
  ${{ runner.arch }}` to keep the two Linux architectures apart, and a
  `sccache --show-stats` step per job.
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

**Alongside `Swatinem/rust-cache`, not instead of it.** That action caches
`target/` wholesale and only writes on `master`; sccache is what gives a PR
branch per-unit hits when it misses that cache.

**`vm-tests.yml` left alone** — its expensive compile happens inside a QEMU
guest, which cannot reach the runner's cache service.

## Measured

`tddy-core` and its dependency tree on an M-series laptop, `local` backend:
cold **62s** (278 units cached, 0 non-cacheable), then **26s** from an emptied
`target/` at a **100% hit rate**.
