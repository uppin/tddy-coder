# 2026-10-05 — The CI scripts' own tests are not run by CI

**Category:** Missing test (CI gate)
**Source:** `#e2e-leg` 5/5 (PR #582), `/pr-wrap` validation

`.github/workflows/ci.yml` runs one script test, `./dev bun test ./scripts/generated-code.test.ts`
(the `Generated code` job's "Drift gate self-test"). The tests of three other CI scripts run only
when someone runs them locally:

- `scripts/nextest-serial-groups.test.ts` — the drift check that no binary starting the LiveKit
  testkit is put back in a serial nextest test-group, and that every name a serial group lists
  exists. Today it guards `.config/nextest.toml` only if someone runs it locally.
- `scripts/ci-e2e-timing.test.ts` — the per-binary e2e timing report (#e2e-leg 3/5).
- `scripts/livekit-ci-server.test.ts` — the shared LiveKit server script and the `slow-timeout`
  hang guard (#e2e-leg 4/5).

**Why deferred:** the developer chose, in the #582 wrap, to keep that PR's CI workflow unchanged and
word the docs as "run locally" instead.

**What closing it takes:** one step in the `Generated code` job, which already installs bun, e.g.
`./dev bun test ./scripts/nextest-serial-groups.test.ts ./scripts/ci-e2e-timing.test.ts ./scripts/livekit-ci-server.test.ts`
then drop the "run locally" caveat from
`docs/dev/guides/ci.md` and the comment above the LiveKit override in `.config/nextest.toml`.
