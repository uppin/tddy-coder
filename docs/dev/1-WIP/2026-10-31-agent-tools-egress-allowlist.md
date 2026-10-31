# Changeset: A tool declares the hosts it may reach, and the jail enforces it

**Created:** 2026-10-31
**Status:** 🚧 In Progress
**Type:** Feature (security)
**Complexity**: 4 — three crates (`tddy-actions`, `tddy-sandbox`, `tddy-sandbox-runner`), two enforcement points (the shim and the host relay), a live jail test
**Stack:** `#agent-tools 3/12` — branch `feature/agent-tools/egress-allowlist`, base `feature/agent-tools/claude-host`
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

## Responsibility

- `tddy_actions::tool::{NetworkSection { egress: Egress, oauth_inbound: bool }, Egress::{None, Any, Hosts(Vec<HostPattern>)}, HostPattern::parse}`. Patterns are `host[:port]` and `*.domain[:port]`, and the port defaults to 443. A missing section means `None`.
- `tddy_sandbox::network::EgressPolicy::{None, Any, Hosts(Vec<HostPattern>)}` and `EgressPolicy::admits(host, port) -> bool`. `NetworkSpec.egress`.
- `tddy_sandbox_recipes::tool_grants::lower_network(&[ResolvedTool]) -> (EgressPolicy, bool)`, the union rule: `Any` dominates, `Hosts` concatenate, `None` contributes nothing.
- The runner's egress shim calls `admits` per `CONNECT` and answers `403` with the host. The shim does not start when the policy is `None`.
- `run_host_relay`'s `TunnelOpen` calls `admits` independently of the shim. A refusal is logged with the session id, the host and the declaring tool set.

## Boundaries

- No coding agent declares hosts here. Agent jails keep `Any` until #5, #7 and #8.
- No path or method filtering, no DNS pinning, no TLS inspection.
- Does not touch `claude.yaml` or `tddy-coding-agent-specs` (#2).

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` tool-defs (#790) | `ToolDef`, the registry, `ResolvedTool` and `tool_grants::lower_tools` | adds `network` to `ToolDef`, and adds `lower_network` beside `lower_tools` | change `lower_tools`, `ResolvedTool`'s existing fields, or the registry's override rule |
| `n2` claude-host | nothing consumed | sits above #2 only because of the line | touch `run`, `extensions`, specs or slots |

## Draft PR contract

The first push publishes `NetworkSection`, `Egress`, `HostPattern`, `EgressPolicy` (with `admits` as `// TODO(egress-allowlist): implement`) and `lower_network`, plus the runner and relay call sites that return `admits`' result, and the failing tests below.

## Green wave

**Wave:** 2 of 7
**Greenable independently:** yes, once #1 is green. Tests run a generic tool (`curl`) in a real jail against local listeners and need no agent
**Concurrent with:** #2 claude-host and #4 build-tools
**Blocks:** #5 claude-jail, #7 cursor-cli and #8 harness-clis, whose specs declare hosts and whose tests assert an undeclared host is refused

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

- [ ] `a_tool_declaring_one_local_host_reaches_it_through_the_shim_and_another_local_port_is_refused_403`
- [ ] `a_tunnel_open_sent_straight_to_the_host_relay_for_an_undeclared_host_is_refused`
- [ ] `a_jail_whose_tools_declare_egress_none_starts_no_shim_and_has_no_proxy_env`
- [ ] `one_tool_declaring_any_makes_the_jails_policy_any`
- [ ] `a_refused_host_is_logged_with_the_tool_set_that_lacked_it`

**`tddy-actions`: `packages/tddy-actions/tests/tool_registry_acceptance.rs` (extended)**

- [ ] `a_malformed_host_pattern_is_refused_at_load_naming_the_tool`

### Unit / integration tests

- [ ] `tddy-sandbox` `network`: `admits` for an exact host, a host with a port, the default port 443, a wildcard subdomain that does not match the apex, a case-insensitive host, and an IPv4 literal.
- [ ] `tddy-sandbox-recipes` `lower_network`: the union rule with every combination of `None`, `Any` and `Hosts`; `oauth_inbound` is true if any tool declares it.

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
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
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
