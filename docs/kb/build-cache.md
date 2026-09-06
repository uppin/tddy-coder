# Build cache (sccache) — what was tried, what broke, what it cost

Field notes from wiring a per-host sccache build cache into this repo and into
CI (PR #452, 2026-09-06). The guide — [docs/dev/guides/build-cache.md](../dev/guides/build-cache.md) —
documents how the thing works *now*. This documents how it got there: the dead
ends, the failure modes that look like something else, and the measurements
behind each decision.

Read this before changing the CI cache wiring, or before concluding a cache is
"just cold".

## The measurements, all in one place

Every hit rate below is from `sccache --show-stats`, not from wall time.

| Backend | Where | Units | Hit rate | Read latency |
|---|---|---|---|---|
| `local` | laptop, `tddy-core` + deps | 278 | **100%** | ~0.01s |
| `redis` | laptop → Pi over TLS | 278 | **100%** | 0.666s |
| `redis` | CI, `Rust lint` x64 | 66 | **96.97%** | — |
| `redis` | CI, `Rust build` x64, cold `target/` | 1553 | **97.02%** | 0.587s |
| `redis` | CI, `Rust build (arm64)`, cold `target/` | 1527 | 60.38% | 0.481s |
| `github-actions` | CI, `Rust build (arm64)` | 64 | **100%** | 0.191s |

Wall times for `Rust build` (x86_64):

| | `rust-cache` restore | `cargo build` | job total |
|---|---|---|---|
| Healthy master run | 40–67s (real restore) | **133–140s** | 5–6 min |
| With `rust-cache` missing, sccache at 97% | 4s (`No cache found.`) | **513s** | 15 min |
| Estimated: neither cache | — | ~860s | — |

**The lesson in that table:** sccache at a 97% hit rate is still ~3.7× slower
than a warm `Swatinem/rust-cache` archive. A per-object cache does 1498 network
round trips; a `target/` archive does one. sccache is a *fallback* for when
rust-cache misses, never a replacement for it.

## Failure modes that do not look like failures

Three of these cost a full CI round trip each, because from the outside every
one of them is indistinguishable from "the cache is cold".

### 1. `runner` context in a job-level `env:` — invalid workflow, zero jobs

```yaml
    env:
      SCCACHE_GHA_VERSION: ${{ runner.arch }}   # rejected
```

A job's `env:` is evaluated before a runner is assigned, so it has no `runner`
context. This is not a bad value — GitHub refuses to parse the workflow, starts
**no jobs at all**, and the check goes red with nothing inside it to read. The
run failed in under a minute with an empty job list.

Set it from a step instead (`process.env.RUNNER_ARCH` inside `github-script`).
Guarded by `no_job_level_env_reaches_for_the_runner_context`, which scans every
workflow in the repo.

### 2. Missing `ACTIONS_CACHE_SERVICE_V2` — every read and write 404s

```
uri: https://results-receiver.actions.githubusercontent.com/_apis/artifactcache/caches
called: Backend::ghac_reserve
=> 404 page not found
```

`_apis/artifactcache/` is the **v1** cache API, which GitHub decommissioned;
`results-receiver` is the **v2** host. sccache picks its protocol from
`ACTIONS_CACHE_SERVICE_V2`; without it, it builds v1 paths and aims them at the
v2 endpoint. Exporting `ACTIONS_RESULTS_URL` and `ACTIONS_RUNTIME_TOKEN` is not
enough.

**Two consecutive runs both reported 0% and looked like an ordinary cold cache.**
The only tell was `Cache write errors 64` buried in the stats block. Set
`SCCACHE_LOG=warn` and `SCCACHE_ERROR_LOG` and print the log beside the stats, or
this class of bug is invisible.

### 3. Comparing runs across different cache scopes proves nothing

GitHub scopes Actions cache entries by ref. A `workflow_dispatch` run
(`refs/heads/<branch>`) and a `pull_request` run (`refs/pull/N/merge`) are
**sibling scopes that cannot read each other**:

```
refs/heads/feature/per-host-build-cache: 72 entries
refs/pull/452/merge:                     25 entries
```

Alternating `gh workflow run` with `git push` produced a 0% hit rate that had
nothing to do with the cache working. **A hit test needs two runs of the same
event on the same ref.**

### 4. `SCCACHE_REDIS` prints the password into the build log

sccache's single-URL form is deprecated *and* echoes what it was given:

```
Cache location  redis, name: rediss://:hunter2@host:16380, prefix: /tddy-ci/
```

Use `SCCACHE_REDIS_ENDPOINT` + `SCCACHE_REDIS_PASSWORD` instead. The stats line
then carries the endpoint alone, which is what makes a per-job `sccache stats`
step safe to keep. Endpoint is not secret; password is a repo secret.

### 5. The cache key includes the absolute target directory path

Measured: identical source, only `CARGO_TARGET_DIR` different → **100% hits vs
0%**. Rust compilations carry `--out-dir` and `-L dependency=` paths into the
hash.

Consequences, all counter-intuitive:

- Two `.worktrees/*` checkouts share **nothing**.
- A laptop and a CI runner share **nothing**.
- What does share: same-path rebuilds, a wiped `target/`, branch switches inside
  one checkout, and runner-to-runner (their path is stable).

An early draft of the guide claimed the cache helps "whenever a worktree is
created". It does not. Pointing every worktree at one `CARGO_TARGET_DIR` would
fix that case; the repo does not do this today.

## Why CI does not use the GitHub Actions cache

It works. It was measured at a **100% hit rate**, taking `Rust build (arm64)`
from 4m02s to 2m56s. It was still the wrong choice here, for a reason that has
nothing to do with sccache's correctness.

**GitHub caps a repository's Actions cache at a hard 10 GB**, and sccache stores
one entry per compilation unit. Two runs wrote **1960 entries (1.5 GB)**.

Cache entries are never overwritten — keys are immutable. `Swatinem/rust-cache`
mints *one* new archive when `Cargo.lock` changes and the old one ages out;
sccache mints new content-hashed entries for every touched unit and nothing ever
replaces them. Usage only ratchets upward between evictions.

### The underlying problem: this repo is already over budget

Steady-state requirement, measured:

| Entry | Size |
|---|---|
| `v0-rust-build-Linux-x64` | 2616 MB |
| `v0-rust-test-Linux-x64` | 1616 MB |
| `nix-Linux` | 1613 MB |
| `v0-rust-build-arm64` | 1566 MB |
| `nix-Linux-ARM64` | 1530 MB |
| `v0-rust-lint-Linux-x64` | 1145 MB |
| `v0-rust-vm-Linux-x64` | 579 MB |
| debian image + cypress ×2 | 657 MB |
| **total** | **≈ 11.3 GB against a 10 GB cap** |

Every master run evicts something. `ci.yml`'s `save-if: master` rules were meant
to prevent exactly this and do not. The sccache experiment tipped it over and
cost the 2.6 GB `v0-rust-build-Linux-x64` archive, but the repo was ~13% over
before any of this started.

**Four of those archives are x86_64 `target/` directories holding the same ~57
dependency crates compiled identically** — `lint`, `test`, `build` and `vm` use
separate `shared-key`s totalling 5956 MB. Consolidating the x64 keys is the
obvious headroom, and is untried as of this writing.

### How eviction actually works

1. **7 days without being _accessed_** — not since creation. An entry nothing
   reads dies in a week; one that keeps getting restored lives indefinitely.
2. **The 10 GB ceiling, least-recently-used** — silent, no warning.
3. Manual deletion.

## Redis: works, but latency is the ceiling

CI now caches through Redis, which keeps the build cache off the Actions budget
entirely. Verified: reachable from runners over TLS, secret resolves, zero read
or write errors, password absent from logs.

**The open problem is latency.** 0.587s per cache read to a home Raspberry Pi
over the public internet, versus 0.191s for GitHub's cache and ~0.01s for local
disk. On a cold `target/`: 1498 hits × 0.587s = **879s of Redis wait**, roughly
220s of wall time across 4 vCPUs, and the dominant cost inside `cargo build`.

That is tolerable when rust-cache is warm (~64 units, maybe +15s). It dominates
exactly when sccache is meant to help.

Options not yet tried, roughly in order of promise:

1. **Consolidate the x64 `rust-cache` shared-keys.** Fixes the eviction that
   caused every symptom here. Costs nothing. Do this first.
2. **`SCCACHE_DIR` + one `actions/cache` entry.** One archive, one round trip,
   local-speed reads, size-bounded by `SCCACHE_CACHE_SIZE` — it fixes both flaws
   that killed the per-object GHA backend (1960 entries → 1; 0.191s → 0.01s).
   Needs budget headroom from (1) first.
3. **Host the object store near the runners.** Runners are in Virginia
   (`x-github-edge-region: iad`). S3/R2 or a cloud Redis there would cut 587ms to
   ~20ms. No Actions budget impact; needs infra and an `s3` backend in the
   resolver.

Also unaddressed: `Reclaim runner disk` costs **36–94s on every healthy build**
and hit 277s once. That is a bigger line item than anything sccache changed.

## Operational notes

- **Redis needs `maxmemory` + `maxmemory-policy allkeys-lru`.** It has no
  equivalent of GitHub's 7-day expiry, so entries accumulate until the disk
  fills. Two architectures × three job types write concurrently, ~1500 objects
  per arch per cold build.
- **Jobs share a key prefix on purpose.** `Rust lint` runs `clippy-driver` and
  `Rust build` runs `rustc`, so workspace crates are cached separately — but
  `cargo clippy` builds *dependencies* with plain rustc, so those objects are
  shared across lint, build and test. Measured: 96.97% and 97.02% on the same
  key space. Namespacing per job would make each pay for its own dependency tree.
- **`CARGO_INCREMENTAL=0` is mandatory, not a preference.** sccache declines any
  unit carrying `-C incremental`, so cargo's dev-profile default would leave the
  cache permanently empty. It is a real local trade-off: incremental wins on a
  tight edit-rebuild loop in one crate, sccache wins on branch switches and
  wiped `target/`s.
- **Deleting caches needs a working loop.** `for id in $ids` silently ran once
  instead of 92 times because `IFS` carried a stray NUL byte; the deletes then
  "failed" and were misdiagnosed twice — first as a missing `actions:write`
  scope, then as rate limiting. Use `while IFS= read -r id`, and verify a
  destructive loop's iteration count before blaming the API.
- **Do not test a destructive API call on an unfiltered first result.** Doing so
  deleted a 1.5 GB `rust-cache` archive that CI depends on.

## Can sccache cache test *runs*?

No. sccache caches compiler invocations; it has no concept of executing a binary.
It already speeds up the build half of `cargo nextest run`, which is ~4 min of a
25 min test job.

Skipping unchanged test *execution* would need either nextest filtersets
(`-E 'rdeps(<pkg>)'` plus `git diff`) or a result cache keyed on the test binary
hash. **Both are unsafe in this repo today**: many suites exec workspace binaries
by path (`Command::new("target/debug/tddy-daemon")`), a dependency the cargo
graph cannot see — `ci.yml` already documents this when explaining why it ships
fixture binaries as artifacts. Change `tddy-daemon` and neither `rdeps()` nor a
binary hash would invalidate the suite that execs it, producing a green check
from tests that never ran against the change.

The safe lever for that 25 min is **sharding** — `cargo nextest run --partition
count:1/4` across parallel jobs — which costs runner minutes instead of
correctness. Fix the invisible-dependency problem before considering either
caching approach.
