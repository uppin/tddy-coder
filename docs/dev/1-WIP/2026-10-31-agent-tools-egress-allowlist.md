# Changeset: A tool declares the hosts it may reach, and the jail enforces it

**Created:** 2026-10-31
**Status:** 🚧 In Progress
**Type:** Feature (security)
**Complexity**: 4 — three crates (`tddy-actions`, `tddy-sandbox`, `tddy-sandbox-runner`), two enforcement points (the shim and the host relay), a live jail test
**Stack:** `#agent-tools 3/12` — branch `feature/agent-tools/egress-allowlist`, base `feature/agent-tools/claude-host`
**PR:** [#792](https://github.com/uppin/tddy-coder/pull/792)
**PRD:** [PRD-2026-10-31-tool-egress-allowlist.md](../../ft/coder/1-WIP/PRD-2026-10-31-tool-egress-allowlist.md)

## Initial Discovery

[2026-10-31-agent-tools-egress-allowlist-initial-discovery.md](./2026-10-31-agent-tools-egress-allowlist-initial-discovery.md)

State A below is distilled from that file. Grep traces and file dumps are not repeated here.

## Prerequisites

| Record | Verdict |
|---|---|
| `packages/tddy-sandbox-runner/docs/code-issues/oversized-file-runner.md`, `oversized-file-host-relay.md`, `complexity-host-relay-run-host-relay-inner.md` | ⚠ During. The policy check lives in a new `egress_policy.rs`. `runner.rs` and `host_relay.rs` gain a call each, not a branch tree |
| `packages/tddy-sandbox/docs/code-issues/oversized-file-builder.md` | ⚠ During. `EgressPolicy` goes in `network.rs`, not `builder.rs` |

## Affected Packages

- [ ] [`tddy-actions`](../../../packages/tddy-actions/README.md): `ToolDef.network { egress, oauth_inbound }`; pattern validation
- [ ] [`tddy-sandbox`](../../../packages/tddy-sandbox/README.md): `NetworkSpec.egress: EgressPolicy`; plan serialisation to the runner
- [ ] [`tddy-sandbox-recipes`](../../../packages/tddy-sandbox-recipes/README.md): `tool_grants` lowers the union of a tool set's network declarations
- [ ] [`tddy-sandbox-runner`](../../../packages/tddy-sandbox-runner/README.md): the shim refuses undeclared `CONNECT`s; the host relay refuses undeclared `TunnelOpen`s; refusals are logged
- [ ] [`tddy-daemon-sandbox`](../../../packages/tddy-daemon-sandbox/README.md): `build_workspace_tool_plan` lowers its tools' network into `NetworkSpec.egress` and the runner's `--egress-shim-port` / `--egress-policy` / `--egress-tool` (green; the acceptance tests pin it)
- Mechanical only (a new field in a struct literal): `tddy-sandbox-darwin`, `tddy-sandbox-qemu` (`NetworkSpec.egress`), `tddy-sandbox-app` (`HostRelayConfig.egress`), `tddy-integration-tests` and `tddy-sandbox-darwin` tests (`SandboxRunnerArgs`), `tddy-agent-launch` tests (`ResolvedTool.network`)

## Responsibility

- `tddy_actions::tool::{NetworkSection { egress: Egress, oauth_inbound: bool }, Egress::{None, Any, Hosts(Vec<HostPattern>)}, HostPattern::parse}`. Patterns are `host[:port]` and `*.domain[:port]`, and the port defaults to 443. A missing section means `None`.
- `tddy_sandbox::network::EgressPolicy::{None, Any, Hosts(Vec<HostPattern>)}` and `EgressPolicy::admits(host, port) -> bool`. `NetworkSpec.egress`.
- `tddy_sandbox_recipes::tool_grants::lower_network(&[ResolvedTool]) -> (EgressPolicy, bool)`, the union rule: `Any` dominates, `Hosts` concatenate, `None` contributes nothing.
- The runner's egress shim calls `admits` per `CONNECT` and answers `403` with the host. The shim does not start when the policy is `None`.
- `run_host_relay`'s `TunnelOpen` calls `admits` independently of the shim. A refusal is logged with the session id, the host and the declaring tool set.

## Boundaries

- No coding agent declares hosts here. Agent jails keep `Any` until #794, #796 and #797.
- No path or method filtering, no DNS pinning, no TLS inspection.
- Does not touch `claude.yaml` or `tddy-coding-agent-specs` (#791).

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` tool-defs (#790) | `ToolDef`, the registry, `ResolvedTool` and `tool_grants::lower_tools` | adds `network` to `ToolDef`, and adds `lower_network` beside `lower_tools` | change `lower_tools`, `ResolvedTool`'s existing fields, or the registry's override rule |
| `n2` claude-host | nothing consumed | sits above #791 only because of the line | touch `run`, `extensions`, specs or slots |

## Draft PR contract

The first push publishes `NetworkSection`, `Egress`, `HostPattern`, `EgressPolicy` (with `admits` as `// TODO(egress-allowlist): implement`) and `lower_network`, plus the runner and relay call sites that return `admits`' result, and the failing tests below.

## Green wave

**Wave:** 2 of 7
**Greenable independently:** yes, once #790 is green. Tests run a generic tool (`curl`) in a real jail against local listeners and need no agent
**Red-phase edges with #790 (2026-10-31):** `a_malformed_host_pattern_is_refused_at_load_naming_the_tool` needs #790's green `ToolRegistry::load`; and #790's green `resolve_closure` must carry `ResolvedTool.network` once this branch is rebased onto it (n3 green adds `network: def.network.clone()`). Both are already implied by n1 → n3.
**Concurrent with:** #791 claude-host and #793 build-tools
**Blocks:** #794 claude-jail, #796 cursor-cli and #797 harness-clis, whose specs declare hosts and whose tests assert an undeclared host is refused

Real dependency edges, as opposed to the branch line:

    n1 → n2, n3, n4      n2 → n5, n6      n3 → n5, n7, n8      n5 → n7, n8, n10
    n6 → n9      n7 → n9, n10      n8 → n10, n11      n9 → n11      n2..n11 → n12

## State A

- `NetworkSpec { loopback_allow_ports, allow_oauth_inbound }`. Egress exists whenever the runner gets `--egress-shim-port`.
- The shim is a loopback CONNECT proxy. `host_relay` `spawn_tunnel` dials any `host:port`, and no allowlist exists anywhere.
- `allow_oauth_inbound` is set by `SandboxRecipe` for unrouted Claude and Cursor.

## State B

- A jail lowered from tools carries `EgressPolicy`. The shim and the relay both enforce it, so an undeclared host is refused twice, independently.
- Recipe-built agent jails pass `EgressPolicy::Any`, which is today's behaviour, until their nodes.

## Delta

### New
- `packages/tddy-sandbox/src/network.rs`
- `packages/tddy-sandbox-runner/src/egress_policy.rs`
- `packages/tddy-actions/src/tool/network.rs`
### Modified
- `tddy-sandbox` `NetworkSpec` and the runner plan or args carry the policy.
- `runner.rs` (shim) and `host_relay.rs` (`TunnelOpen`) each call it once.
- `tool_grants.rs` gains `lower_network`.
- Docs at wrap: `docs/ft/coder/sandbox-builder.md` § Network; `packages/tddy-sandbox-darwin/docs/troubleshooting.md` § CONNECT tunnel.

## Testing Strategy

### Acceptance tests

**`tddy-daemon-sandbox`: `packages/tddy-daemon-sandbox/tests/tool_egress_allowlist_acceptance.rs`**

- [x] `a_tool_declaring_one_local_host_reaches_it_through_the_shim_and_another_local_port_is_refused_403` — the real runner in-process (generic-PTY mode) with the real host relay; `curl --proxytunnel` from the test against two loopback servers
- [x] `a_jail_whose_tools_declare_egress_none_starts_no_shim_and_has_no_proxy_env`
- [x] `a_jail_whose_tool_declares_hosts_passes_its_runner_exactly_those_hosts_and_the_tool_set` (positive control for the one above)
- [x] `one_tool_declaring_any_makes_the_jails_policy_any`

**`tddy-sandbox-runner`: `packages/tddy-sandbox-runner/tests/egress_allowlist_relay_acceptance.rs`** (moved: the fake jail that sends a raw `TunnelOpen` lives in that crate's `tests/common`)

- [x] `a_tunnel_open_sent_straight_to_the_host_relay_for_an_undeclared_host_is_refused`
- [x] `a_tunnel_open_for_a_declared_host_is_dialed_under_a_host_policy` (positive control)
- [x] `a_refused_host_is_logged_with_the_tool_set_that_lacked_it`

**`tddy-actions`: `packages/tddy-actions/tests/tool_registry_acceptance.rs` (extended)**

- [x] `a_malformed_host_pattern_is_refused_at_load_naming_the_tool`

### Unit / integration tests

- [x] `tddy-sandbox` `network` (in-module): `admits` for an exact host, a host with a port, the default port 443, a wildcard subdomain that does not match the apex, a case-insensitive host, and an IPv4 literal; an empty host list; the `--egress-policy` round trip and its refusal.
- [x] `tddy-actions` `tool::network` (in-module): `HostPattern::parse` (default 443, wildcard + port + lowercase, scheme, inner `*`, port 0, empty label) and a `network:` section in all three forms.
- [x] `tddy-sandbox-runner` `runner` (in-module): the shim answers an undeclared `CONNECT` `403` naming it and opens no tunnel; it tunnels a declared one (case-insensitively); the runner reads `--egress-policy` / `--egress-tool`.
- [x] `tddy-sandbox-recipes` `lower_network` (`tests/lower_network_unit.rs`): the union rule with every combination of `None`, `Any` and `Hosts`; `oauth_inbound` is true if any tool declares it.

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| Pattern matching, union | Unit | Pure |
| Shim and relay enforcement | Integration in a real jail against loopback listeners | Both enforcement points must be shown independently; a mock relay would not prove the bypass is closed |

## Decisions & Trade-offs

- **Enforced at both the shim and the relay.** The shim alone can be bypassed by a jailed process speaking the relay protocol directly.
- **A missing `network:` means `none`.** A tool that needs the network says so. This is a declared default, not a fallback.
- **The union rule makes `any` dominant.** It is the honest reading of a jail that hosts one unrestricted tool.

## TODO

- [x] Record initial discovery (`2026-10-31-agent-tools-egress-allowlist-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests — waived by the developer 2026-10-31: pass 2 runs without per-node review
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap

## Refactoring Needed
### From /red (TDD red phase)

**Red baseline (2026-10-31, contract commit).** Every new test compiles and fails; each failure is a `todo!("egress-allowlist: …")` of this node, or an assertion on behaviour it owns:

| Test | Fails on |
|---|---|
| `tddy-sandbox` `network::tests` (9) | `EgressPolicy::admits` (Hosts arm), `to_runner_arg` / `from_runner_arg` (hosts form) |
| `tddy-actions` `tool::network::tests` (7) | `HostPattern::parse`, `Egress::deserialize` |
| `tddy-sandbox-recipes` `lower_network_unit` (7) | `lower_network`, `HostPattern::parse` |
| `tddy-sandbox-runner` `runner::tests` egress shim (3) | `EgressPolicy::admits`, `from_runner_arg` |
| `tddy-sandbox-runner` `egress_allowlist_relay_acceptance` (3) | `admits` panics in the relay's reader task, so no `TunnelOpenAck` arrives and the ack assertions fail |
| `tddy-daemon-sandbox` `tool_egress_allowlist_acceptance` (4) | `HostPattern::parse` in the Given; the `none` jail fails its `plan.network.egress == None` assertion (the workspace plan is still `Any` and has no shim) |
| `tddy-actions` `a_malformed_host_pattern_is_refused_at_load_naming_the_tool` | **`tool-defs: ToolRegistry::builtin`** — a parent TODO. Sequencing fact: it can only turn green once #790's registry load is green; it then exercises `Egress::deserialize` and `HostPattern::parse` (this node) |

**Pre-existing failures in touched packages (parent TODOs, not this node):** `tddy-actions` lib 9× `claude-host: render_argv`, and its registry/resolve suites on `tool-defs: ToolRegistry::builtin` / `resolve_closure`; `tddy-sandbox-recipes` `tool_grants_unit` 6× `tool-defs: lower_tools`. `tddy-daemon-sandbox/tests/sandbox_stdio_seatbelt_acceptance.rs` does not compile on macOS (excluded, known).

**Deviations from the planned surface and tests, and why:**

- **Two `HostPattern`s.** `tddy_actions::tool::HostPattern` (serde, `parse`, port default) and `tddy_sandbox::HostPattern { host, port }` (plain, matched by `admits`). `tddy-sandbox` has no serde and must not learn the tool vocabulary; `tddy-actions` must not depend on the sandbox crate. `lower_network` converts.
- **`ResolvedTool` gains `network: NetworkSection`** (additive; the changeset's `lower_network(&[ResolvedTool])` needs it). Five test literals were updated (`network: Default::default()`). **Green edge:** #790's green `resolve_closure` builds `ResolvedTool` without the field, so when this branch is rebased onto it, n3's green must add `network: def.network.clone()` there.
- **`Egress` has hand-written serde.** serde_yaml 0.9 reads a derived enum only as a YAML tag (`!hosts [...]`), never the PRD's `{ hosts: [...] }` map — verified. `Serialize` is real; `Deserialize` is this node's `todo!`.
- **The runner's policy travels as `--egress-policy <none|any|hosts:h:p,…>` plus `--egress-tool <id>` (repeated)**, defaulting to `any` (today's behaviour). `SandboxRunnerArgs` literals in `tddy-sandbox-darwin` (2) and `tddy-integration-tests` (1) gained the two fields.
- **`HostRelayConfig` gains `egress: EgressGuard`** (default `EgressGuard::unrestricted`, builder `with_egress`); `tddy-sandbox-app`'s struct literal passes `unrestricted`.
- **`EgressPolicy::default()` is `Any`.** `NetworkSpec` derives `Default` and is built that way by the darwin/cgroups backends and tests; `Any` keeps every existing jail as it is. A tool-lowered plan always sets its own. **Developer decision:** keep `Any` as the `NetworkSpec` default, or make the field required (touches every backend).
- **`admits`' `None`/`Any` arms and `to/from_runner_arg`'s `none`/`any` forms are implemented**, not `todo!`: every existing jail runs with `any`, and both call sites sit on the production CONNECT path, so a `todo!` there would break the currently-green tunnel tests (`host_relay_dispatch`, darwin `sandbox_runner_behavior_acceptance`, the runner's shim tests).
- **The shim check is in `open_relay_tunnel`**, so it covers the plain-HTTP forward path as well as `CONNECT`.
- **Acceptance tests are split across two crates and not run under Seatbelt.** The relay-bypass tests need the fake jail in `tddy-sandbox-runner/tests/common`; the shim test runs the real runner in-process with the real relay and `curl`. Enforcement is the runner's code, not the backend's, and this way CI (ubuntu) runs them. `a_refused_host_is_logged_…` asserts the refusal line on the relay's `TunnelOpenAck` error, which is the same `EgressRefusal` `Display` the relay `log::warn!`s and the shim appends to the session's `sandbox-runner.log`.
- **Parent test fixture edited:** `tool_registry_acceptance.rs::an_unknown_field_is_refused_naming_the_file_and_the_field` (#790) used `network: any` as its "unknown field"; with `network` now a real field it changes to `colour: blue`.
- **`lower_network` de-duplicates hosts** (case-insensitively, after the 443 default) — a decision the plan left open.

**Left for green (`TODO(egress-allowlist)` in code):** start no shim and set no proxy env when the policy is `None` (`run_sandbox_runner`); refuse a `TunnelOpen` whose port is not a `u16` outright (`host_relay`); wire `build_workspace_tool_plan` to `lower_network` (shim port, `--egress-policy`, `--egress-tool`, `NetworkSpec.egress`, `allow_oauth_inbound`).
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
| Validation | Last Run | Status |
|------------|----------|--------|
| /validate-changes | Not yet run | Pending |
| /validate-tests | Not yet run | Pending |
| /validate-prod-ready | Not yet run | Pending |
| /analyze-clean-code | Not yet run | Pending |
