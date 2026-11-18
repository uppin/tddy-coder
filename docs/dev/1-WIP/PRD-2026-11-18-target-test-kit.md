# PRD: Target-session tests share one stand-in kit, and a live test proves the Coder path on a real deployment

**Date**: 2026-11-18
**Status**: 🚧 In Progress
**Stack**: `#coder-start` 9/10 — branch `feature/coder-start/target-test-kit`, base `feature/coder-start/start-visibility`

## Problem

1. **The target-session fixtures are copied across 11 test files.** The copies include the stand-in
   ssh/coder target, the stub agents, the `acme` agent, the daemon helpers and the guarded
   `write_executable` warm-up. They live in 3 crates. Any change to how a target session starts
   has to be made in every copy. The stack's own nodes 5–8 add more.
2. **The Coder path has never been run end to end against a real deployment by a test.** The
   facts behind the stand-ins have not been checked against a real deployment by any test:
   - the `coder version` output the host probe parses;
   - the workspace status values;
   - the hook follower on a workspace;
   - the start of Claude and Cursor there.

   The live runs on 2026-11-18 were done by hand.

## Requirements

1. **One stand-in kit.**
   - A `tddy-testing-commons` module holds the language-neutral parts: `write_executable` with
     the warm-up, a stand-in ssh target and a stand-in `coder` (script body plus root mapping),
     the stub agent scripts, and the shared constants.
   - Each crate keeps a `tests/support/` module for the parts typed in its own crate (the `acme`
     `HostAgent`, the daemon helpers).
   - Every copy is replaced by an import. Assertions do not change.
2. **A live production test in `tddy-session-driver`.**
   - It is `#[ignore = "production test: …"]`, run with `--ignored`.
   - It requires `TDDY_CODER_LIVE_WORKSPACE` plus `CODER_URL`/`CODER_SESSION_TOKEN`. These can be
     exported or come from `.env`. A missing variable fails the test; it never skips.
   - It drives the driver binary against the workspace and checks four things:
     - a Claude session reaches its prompt;
     - a Cursor session mints its chat and reaches its prompt;
     - a streamed hook log is followed;
     - the deployment's `coder version` and workspace status read as the stand-ins assume.
3. **Documentation.** The test is listed in `docs/dev/guides/testing.md` § Production Tests, with
   how to run it.

## Acceptance criteria

- [ ] No target-session test file defines `write_executable`, a stand-in ssh/coder target, a stub
      agent or an `acme` spec of its own. Each test's assertions are unchanged.
- [ ] The touched suites pass with the same names and counts as before.
- [ ] `./dev cargo test -p tddy-session-driver --test coder_live_production -- --ignored` passes
      against `mantasi/remote-dev`. Without the variables, it fails naming the missing one.
- [ ] Scoped gates: `./test -p tddy-testing-commons -p tddy-agent-launch -p tddy-session-lifecycle -p tddy-session-driver`
      and clippy with `-D warnings`.

## Out of scope

- CI runs of the live test (it needs a deployment and a token).
- Fixtures outside the target-session family (`agent_registry_support`, tmux, jail).
