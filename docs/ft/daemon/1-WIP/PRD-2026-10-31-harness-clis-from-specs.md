# OpenCode, Muse Code and Warp CLI sessions launch from their tool definitions - PRD

**Date**: 2026-10-31
**PRD Type**: Technical Improvement
**Stack**: `#agent-tools 8/12` (branch `feature/agent-tools/harness-clis`)

## Affected Features

- **Primary**: [OpenCode CLI session](../opencode-cli-session.md), [Muse CLI session](../muse-cli-session.md) and [Warp CLI session](../warp-cli-session.md). All three launch from specs.
- **Related**: [Harness profiles](../harness-profiles.md). A profile's `harness:` names a spec id.

## Summary

`opencode.yaml`, `muse.yaml` and `warp.yaml` replace the `CliHarness` fn-pointer descriptors (`cli_harness.rs`, `cli_harness_muse.rs`, `cli_harness_warp.rs`). Host sessions use the `2/12` launcher and jailed sessions use the `5/12` launcher. What used to be code becomes spec content:

- **provider routes**: `route: { required_in_jail: true, config_env: OPENCODE_CONFIG, files_dir: provider-route }`;
- **model listing**: `models: { list_command: ["{binary}", "models"], adapter: opencode-models }`;
- **policy**: Muse's no-JIT policy;
- **resume**: Warp's resume-token scraping as `resume: { adapter: warp-resume-token }`;
- **jail egress**: each agent's host allowlist.

`CLI_HARNESSES` is removed, and a session type is resolved to a spec id.

## Background

`CliHarness` is the closest thing today to a declarative tool. It carries static descriptors with fn-pointer argv, a binary env and a `route_config_env`. Its jail path is `svc_start_sandboxed_cli_harness_session`, which is over budget, and the sandbox side lives in the `SandboxRecipe::{OpenCodeCli, MuseCli}` arms. Warp is refused in the jail ([2026-10-24-warp-cli-sandbox.md](../../../dev/todo/2026-10-24-warp-cli-sandbox.md)). The developer decided on 2026-10-31 that it stays refused. `warp.yaml` declares `placements: [host]`, and the refusal becomes spec-driven instead of a missing match arm.

## Proposed Changes

### What's Changing

- Three specs, two adapters (`opencode-models`, `warp-resume-token`), and session-type → spec-id resolution.
- `svc_start_sandboxed_cli_harness_session` and the `CliHarness` table are deleted.
- `cli_harnesses.<type>.binary_path` in daemon.yaml stays as an override until `12/12`.

### What's Staying the Same

- A jailed OpenCode or Muse session still requires a provider account.
- Argv, models, route files and the resume token are unchanged.

## Acceptance Criteria

- [ ] Host and jailed `opencode-cli` and `muse-cli` starts and resumes spawn stubs whose argv, env and route files match their specs.
- [ ] A jailed OpenCode start without a route is refused, as today.
- [ ] A `warp-cli` host session records its resume token through the `warp-resume-token` adapter.
- [ ] A sandboxed `warp-cli` start is refused with `FAILED_PRECONDITION`, naming the spec's placements. The refusal comes from the spec, not a missing match arm.
- [ ] Each jail reaches only its declared hosts plus its route's base-URL host.
- [ ] A harness profile naming `opencode` starts from the spec.

## Out of Scope

- OpenCode ACP (`11/12`).
- The standalone app (`10/12`).
