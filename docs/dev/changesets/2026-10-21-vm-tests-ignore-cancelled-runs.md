# 2026-10-21 — vm-tests wait ignores cancelled CI runs

**Type:** Fix

The `Cloudinit + nix + tddy` leg of `vm-tests.yml` picks the CI run to wait on with `gh run list ... --limit 1`. A cancelled twin run of the same trigger and commit (a draft-gate race) could be that one run, and its `Rust build` never completes successfully, so the wait failed on a PR whose real build was green.

The wait now lists up to ten runs and takes the first whose conclusion is not `cancelled`. No match still means keep waiting.

Docs: `docs/dev/guides/ci.md` (VM tests section).
