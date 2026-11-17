# 2026-11-16 — An orchestrator starts a session on a derived host

**Type:** Feature · `#remote-shell` 11/11, PR [#927](https://github.com/uppin/tddy-coder/pull/927), base `feature/remote-shell/remote-checkout-root`, wave 2 of 2

An orchestrator agent can start a session on an SSH host or a Coder workspace: `DescribeSessionStartOptions` lists the derived hosts the daemon reaches (kind, parent, usable, reason), `start_session` takes `target` — a derived-host id from that list — which the daemon resolves for the **caller** (the calling session's token names whose Coder account is read) into the parent daemon and the host's `SessionTarget`; an unknown id is refused naming the listed ones, an unusable host with its reason. The parity test moves `target` from withheld to offered, and the tool's text says a wildcard `Host` entry is not an alias.

## What changed

- **tddy-tool-engine**: `start_options.rs` gains `DerivedHostChoice` and `StartOptions.derived_hosts` / `derived_hosts_unavailable`; `start_session_args.rs` gains `StartSessionArgs.target` and drops `target` from `INTERNAL_START_FIELDS`; the schema offers `target` as a derived-host id from `DescribeSessionStartOptions.derived_hosts` and states that a wildcard `Host` is not an alias. The two remote-placement offers live in `offer_remote_placement` (`schema.rs`), keeping `schema_offering` at 192 lines, under its recorded 195.
- **tddy-host-service**: `service/derived_host_resolution.rs` — `HostServiceImpl::resolve_derived_host(session_token, id) -> Result<ResolvedDerivedHost, DerivedHostResolutionError>`, listing the caller's derived hosts and picking the named one; unknown ids name the listed ones, unusable hosts carry their reason.
- **tddy-daemon-rpc**: `DescribeSessionStartOptions` fills `derived_hosts` from `ListDerivedHosts`; the project-home start resolves a non-empty `target` before building the request and stamps the resolved parent and target. The exec-tool handler takes its host service late through `attach_host_service` (`families.rs`), attached by the daemon's `runtime/mount.rs` from the wired service; an unattached handler refuses with `FAILED_PRECONDITION` naming the missing wiring rather than answering from a bare service.
- **tddy-daemon**: `runtime/mount.rs` attaches the served host service to the RPC handlers; `tests/orchestrator_target_start_acceptance.rs` proves a served daemon lists the caller's Coder account as a derived host and refuses a start naming it with the listing's reason.
- **tddy-session-lifecycle**: `tests/orchestrator_target_start_acceptance.rs` (new test file only) — a start naming a listed SSH host runs on its parent with the host's target, a Coder host resolves with the caller's own account, an unusable host is refused with its reason.
- **tddy-session-create-web**: the parity fixture `start-session-tool-fields.json` offers `target` (34 fields).
- **Docs**: [project-concept.md](../../ft/daemon/project-concept.md) § Start options and § StartSession state the above.

## Verification

Scoped gates at wrap: `CARGO_INCREMENTAL=0 ./dev cargo test -p tddy-tool-engine -p tddy-host-service -p tddy-daemon-rpc` 1,172 passed, 0 failed; `-p tddy-daemon --test orchestrator_target_start_acceptance` 2 passed; `-p tddy-session-lifecycle --test orchestrator_target_start_acceptance` 3 passed; clippy `-D warnings` for the four touched crates and `cargo fmt --check` clean; web parity `createSessionRequestParity.cy.ts` 2 passing (deps via `./dev bun run local-registry-install`). Whole-workspace health is CI's.

`/validate-changes` (stack gate: base `feature/remote-shell/remote-checkout-root`, already current and leak-free) found two gaps, fixed in the single /green re-entry of this wrap: the exec-tool handler's bare-service fallback (removed; an unattached handler now refuses loudly, pinned by a unit test), and the lifecycle suite's use of the parent's removed `default_remote_repo_root` (moved to the sibling suites' `$HOME`-resolved `.tddy/repos/repo/.worktrees` form). The second validation run found no further gaps.

Pre-existing on macOS under the seatbelt jail (`/bin/cat: illegal option`), failing the same way on the untouched base: `a_write_from_a_jailed_codebase_session_lands_inside_the_checkout`, `a_shell_from_a_jailed_codebase_session_runs_with_the_checkout_as_its_cwd`, `a_read_outside_the_checkout_is_refused_by_the_tool_engines_path_containment`, `a_shell_cannot_reach_outside_the_checkout_either`, `resuming_a_sandboxed_codebase_session_re_provisions_its_jail`, `a_sandboxed_codebase_sessions_tool_call_is_served_by_its_jail`, `a_sandboxed_codebase_sessions_tool_call_is_refused_when_its_jail_is_gone`. In this wrap's unsandboxed shell the full daemon-rpc gate passed with them included.

## File length

No touched non-test source file is at or over 500 production lines over the whole PR range (largest: `project_home.rs` at 408; new `derived_host_resolution.rs` at 130). No split was required, and no stack-shared file was split.

## Code issues

`tddy-tools analyze` (coverage + report) ran on tddy-tool-engine (join 67.9%) and tddy-host-service (76.4%): no function this PR added or changed is a new finding (`resolve_derived_host` CRAP 6.0, complexity 6, covered); top rows are pre-existing, untouched functions. The one record this PR touched, `packages/tddy-tool-engine/docs/code-issues/complexity-start-session-args-schema-schema-offering.md`, was re-measured: `schema_offering` went 195 → 199 with the `target` offer and back to 192 after extracting `offer_remote_placement` — under its recorded size, still open at ~3× the 60-line budget, with the new measurement row recorded. No record was added by this PR and none in a touched package got worse.

## Backlog

Deleted: `docs/dev/todo/2026-11-16-orchestrator-start-tool-cannot-start-on-a-target.md` — resolved here: `target` is offered on the start tool, resolved on the daemon for the caller, and the parity test offers it.

Kept, narrowed: `docs/dev/todo/2026-11-16-coder-hosts-never-run-against-a-real-deployment.md` — nodes 9–11 were planned from a real-deployment run's findings; what still has no real-deployment evidence is the hook follower on a Coder workspace, and the entry now says so.

This PR is the stack's top node. The stack's backlog delta is listed in the PR report for the human; no entry was routed to an extra node from here.
