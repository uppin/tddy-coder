# Build cache (sccache)

Rust compilation dominates the time in this workspace: a cold build is 57 crates
including libwebrtc, and `cargo` throws all of it away whenever `target/` is
wiped, a branch swaps a dependency, or a worktree is created. [sccache][] caches
individual compilation units keyed on their inputs, so that work is done once per
input rather than once per `target/` directory.

Where those cached units live is a property of the **host**, not of the repo —
a laptop wants a directory on its own disk, a shared build box wants Redis, a CI
runner wants the Actions cache. So the repo ships the wiring and the host names
the backend.

## How it is wired

`scripts/build-cache-env.sh` resolves the host's answer and prints it as `export`
lines. Both entrances to the dev shell eval it:

- `./dev` — and therefore `./test`, `./verify`, `./release`, `./vm-tests`,
  `./web-dev`, `./install`, `./publish.sh` and `./run-vm-testkit`, all of which
  shell out to it.
- `.envrc` — the direnv path, so a plain `cargo build` in a direnv-loaded shell
  gets the same answer. Quietly, since it runs on every `cd` into the tree.

Neither holds a copy of the logic. The resolver is the single place that decides,
which is what keeps the two entrances from drifting.

`sccache` itself always comes from the nix dev shell (`flake.nix`), so whether it
is *used* is the only variable.

Precedence:

1. **`TDDY_BUILD_CACHE`** names the backend directly and the YAML is never read.
   This is how CI is configured — a runner has no per-host file.
2. **`~/.tddy/build-cache.yaml`** — the per-host file. Override its path with
   `TDDY_BUILD_CACHE_CONFIG`.
3. **Neither** — sccache is left off. `./dev` prints a one-line notice and the
   build runs uncached.

There is deliberately no implicit local cache. A build cache means gigabytes on
someone's disk and a new way for a build to be wrong; opting in is the host
owner's call, and a machine that never opts in behaves exactly as it does today.

## Setting up a host

Copy the example and pick a backend:

```bash
mkdir -p ~/.tddy && cp build-cache.example.yaml ~/.tddy/build-cache.yaml
```

The file is a `type:` selector plus one coordinates block per backend. Only the
selected block is read, so a configured Redis endpoint can sit there unused while
you work off local disk, and switching back is a one-line edit:

```yaml
sccache:
  type: local          # local | redis | github-actions | off

  local:
    dir: ~/.cache/sccache
    max_size: 20G

  redis:
    url: redis://127.0.0.1:6379
    key_prefix: tddy
```

| `type` | Coordinates | Notes |
|--------|-------------|-------|
| `local` | `dir` (default `~/.cache/sccache`), `max_size` (default `10G`) | A directory on this machine. `~/` is expanded. |
| `redis` | `url` (**required**), `key_prefix` | Shared between machines. `url` may carry credentials; they are redacted from the notice `./dev` prints. |
| `github-actions` | `url`, `token`, `version` — all defaulting to the runner's `ACTIONS_RESULTS_URL` / `ACTIONS_RUNTIME_TOKEN` / `SCCACHE_GHA_VERSION` | Only meaningful on a runner. |
| `off` | — | Explicitly no cache, and no notice about it. |

A config that selects a backend it cannot reach — `redis` with no `url`,
`github-actions` with no credentials — **fails the run**. It does not fall back
to compiling uncached: an sccache that cannot reach its cache still compiles
everything, so the only symptom would be a build that is quietly slow forever.

Check what a host resolved to without starting a build:

```bash
./scripts/build-cache-env.sh     # prints the exports, or the "sccache off" notice
./dev sccache --show-stats       # hit rate, cache size, cache location
```

## CI

`.github/workflows/ci.yml` sets `TDDY_BUILD_CACHE: github-actions` on each of the
four Rust jobs (`rust-lint`, `rust-test`, `rust-build`, `rust-build (arm64)`), so
no per-host file is involved. Two details that are easy to get wrong:

- The cache service's credentials are **not** in a `run:` step's environment. A
  JS action has to re-export them, which is all the `Expose the Actions cache to
  sccache` step does.
- `SCCACHE_GHA_VERSION` is set to `runner.arch`. Both Linux runners report the
  same `runner.os`, and an arm64 object is useless to the x86_64 job.

Each job ends with `sccache --show-stats`, so a cache that has quietly stopped
working shows up as a hit rate in the log rather than as a slow build nobody
attributes.

This sits *alongside* `Swatinem/rust-cache`, which caches `target/` and the cargo
registry wholesale and only writes on `master`. That is what makes sccache worth
having on PRs: a branch that misses the `target/` cache still gets per-unit hits
instead of compiling the workspace from nothing.

`vm-tests.yml` is deliberately not wired. Its expensive compile happens *inside*
a QEMU guest, which cannot reach the runner's cache service.

## `CARGO_INCREMENTAL=0`

Every enabled backend also exports this. It is not a preference: sccache refuses
to cache a compilation carrying `-C incremental`, so leaving cargo's dev-profile
default on would hand it a stream of units it declines and the cache would stay
empty. Incremental compilation and a shared compilation cache solve the same
problem in incompatible ways — with sccache on, the cache is the faster of the
two across the branch switches and fresh worktrees this project actually does.

## When something looks wrong

A build cache is a correctness surface, so start by taking it out of the picture:

```bash
TDDY_BUILD_CACHE=off ./dev cargo build ...    # one run, no cache
```

If that fixes it, the cache is implicated and `~/.cache/sccache` (or the Redis
key space) should be cleared — `./dev sccache --show-stats` reports the location.
If it does not, the cache was not the problem.

`./dev sccache --stop-server` restarts the server, which is the fix when sccache
itself is wedged rather than the objects it holds.

[sccache]: https://github.com/mozilla/sccache
