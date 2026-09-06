# Build cache (sccache)

Rust compilation dominates the time in this workspace: a cold build is 57 crates
including libwebrtc, and `cargo` throws all of it away whenever `target/` is
wiped or a branch swaps a dependency. [sccache][] caches individual compilation
units keyed on their inputs, so that work is done once per input.

**One limit to know up front:** sccache's key for a Rust unit includes the
absolute path of the target directory. Measured — same source, same everything,
only `CARGO_TARGET_DIR` different: 100% hits versus 0%. So a cache is shared
between builds at the *same* path and not otherwise. `cargo clean`, a wiped
`target/`, branch switches within one checkout and runner-to-runner all hit;
two `.worktrees/*` checkouts, or a laptop and a CI runner, do not. Pointing
every worktree at one `CARGO_TARGET_DIR` would fix the worktree case, and is
not something this repo does today.

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
   This is how a runner would be configured — CI has no per-host file. See § CI
   for why this repo's own CI leaves it unset.
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
    endpoint: rediss://cache.internal:16380
    password: hunter2
    key_prefix: tddy
```

| `type` | Coordinates | Notes |
|--------|-------------|-------|
| `local` | `dir` (default `~/.cache/sccache`), `max_size` (default `10G`) | A directory on this machine. `~/` is expanded. |
| `redis` | `endpoint` (**required**), `password`, `username`, `db`, `key_prefix` — each also readable from the matching `SCCACHE_REDIS_*` variable | Shared between machines *at the same target path* (see above). Endpoint and password are separate on purpose: sccache's deprecated single-URL form prints the URL it was given — password and all — in `--show-stats`. |
| `github-actions` | `url`, `token`, `version` — all defaulting to the runner's `ACTIONS_RESULTS_URL` / `ACTIONS_RUNTIME_TOKEN` / `SCCACHE_GHA_VERSION` | Only meaningful on a runner. |
| `off` | — | Explicitly no cache, and no notice about it. |

A config that selects a backend it cannot reach — `redis` with no `endpoint`,
`github-actions` with no credentials — **fails the run**. It does not fall back
to compiling uncached: an sccache that cannot reach its cache still compiles
everything, so the only symptom would be a build that is quietly slow forever.

Check what a host resolved to without starting a build:

```bash
./scripts/build-cache-env.sh     # prints the exports, or the "sccache off" notice
./dev sccache --show-stats       # hit rate, cache size, cache location
```

## CI

CI caches through **Redis**, not the GitHub Actions cache. `ci.yml` names the
backend and its coordinates directly on each of the four Rust jobs — a runner
has no per-host file:

```yaml
    env:
      TDDY_BUILD_CACHE: redis
      SCCACHE_REDIS_ENDPOINT: rediss://raitininkai.ddnsgeek.com:16380
      SCCACHE_REDIS_PASSWORD: ${{ secrets.SCCACHE_REDIS_PASSWORD }}
      SCCACHE_REDIS_KEY_PREFIX: tddy-ci
```

The endpoint is not a secret and lives in the workflow. The password is a repo
secret, and is kept *apart from* the endpoint so it never reaches a build log:
sccache's deprecated single-URL form (`SCCACHE_REDIS`) prints the URL it was
given, password included, in `--show-stats`. Split, that line reads
`redis, name: rediss://raitininkai.ddnsgeek.com:16380` and nothing more — which
is why a `sccache stats` step per job is safe to keep.

Use `rediss://` (TLS). The plain port would put the password on the wire in
cleartext between GitHub's runners and the server.

### Why not the GitHub Actions cache

It works — it was wired up, tested over five runs, and measured at a **100% hit
rate**, taking `Rust build (arm64)` from 4m02s to **2m56s**. It was still the
wrong choice here.

GitHub caps a repository's Actions cache at a hard **10 GB**, and sccache stores
one entry per compilation unit. Two runs wrote **1960 entries (1.5 GB)**. This
repo's cache was already over budget — 11 entries totalling 15.3 GB, which the
`save-if: master` rules were meant to prevent — and the added pressure evicted
the 2.6 GB `v0-rust-build-Linux-x64` archive. That archive is what makes these
jobs finish in four minutes; displacing it means more units compile, so sccache
writes more entries, which evicts more. Redis keeps the build cache off that
budget entirely.

The backend remains supported. To use it in a repo with cache headroom, each
Rust job needs `TDDY_BUILD_CACHE: github-actions` plus:

```yaml
      - name: Expose the Actions cache to sccache
        uses: actions/github-script@v7
        with:
          script: |
            core.exportVariable('ACTIONS_RESULTS_URL', process.env.ACTIONS_RESULTS_URL || '');
            core.exportVariable('ACTIONS_RUNTIME_TOKEN', process.env.ACTIONS_RUNTIME_TOKEN || '');
            core.exportVariable('SCCACHE_GHA_VERSION', process.env.RUNNER_ARCH || '');
            core.exportVariable('ACTIONS_CACHE_SERVICE_V2', 'true');
```

Three things there are not optional, and each cost a CI round trip to find:

- **The credentials need a JS action.** `ACTIONS_RESULTS_URL` and
  `ACTIONS_RUNTIME_TOKEN` are not in a `run:` step's environment.
- **`ACTIONS_CACHE_SERVICE_V2` selects the protocol.** Without it sccache builds
  v1 paths (`_apis/artifactcache/…`) and sends them to the v2 host; since GitHub
  decommissioned v1, *every* read and write returns 404 — visible only as a
  `Cache write errors` counter and a cache that stays empty forever.
- **`SCCACHE_GHA_VERSION` namespaces per architecture**, since both Linux runners
  report the same `runner.os`. It must be set from a step: a job-level `env:` has
  no `runner` context, and naming one there is rejected as an invalid workflow
  file — GitHub starts no jobs at all and the check is red with nothing in it.
  `no_job_level_env_reaches_for_the_runner_context` guards this.

### Verifying a cache actually caches

Two runs of the **same event on the same ref**. GitHub scopes Actions cache
entries by ref, so a `workflow_dispatch` run (`refs/heads/<branch>`) and a
`pull_request` run (`refs/pull/N/merge`) are sibling scopes that cannot read each
other; comparing across the two shows 0% and proves nothing. Redis has no such
scoping, but the same-target-path rule at the top of this guide still applies.

Read the result from the `sccache stats` step, not from the wall time.

`vm-tests.yml` is deliberately not wired. Its expensive compile happens *inside*
a QEMU guest, which cannot reach the cache.

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
