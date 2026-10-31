# A tool declares the hosts it may reach, and the jail enforces it - PRD

**Date**: 2026-10-31
**PRD Type**: Enhancement (security)
**Stack**: `#agent-tools 3/12` (branch `feature/agent-tools/egress-allowlist`)

## Affected Features

- **Primary**: [Sandbox builder](../sandbox-builder.md). A plan carries an egress policy, and the host relay enforces it.
- **Related**: [Claude CLI session § Sandbox mode](../../daemon/claude-cli-session.md). Today any jailed process can reach any `host:port` through the runner's CONNECT shim. After this node, a jail whose tools declare hosts reaches only those.

## Summary

`ToolDef` gains a **`network:`** section, which is generic for any tool:

```yaml
network:
  egress: none            # no shim at all
  # egress: any           # today's behaviour, now stated explicitly
  # egress: { hosts: ["api.anthropic.com:443", "*.statsig.com:443"] }
  oauth_inbound: false    # loopback inbound for an OAuth callback
```

The jail's effective policy is the **union of its tool set's declarations**:

- `none` contributes nothing;
- `any` makes the whole jail `any`;
- host lists are combined.

Two components enforce it:

- **The runner's CONNECT shim** refuses a `CONNECT` to an undeclared host and returns `403`, naming the host.
- **The host relay's `TunnelOpen`** refuses the dial independently, so a jail cannot bypass the shim by talking to the relay directly.

A refused connection is logged and counted against the session, so an operator can see which host a tool needed.

## Background

`NetworkSpec` has `loopback_allow_ports` and `allow_oauth_inbound`. Today:

- egress is on whenever the runner is given `--egress-shim-port`;
- `spawn_tunnel` dials whatever the jail asks for, with no allowlist anywhere;
- `allow_oauth_inbound` is decided by the recipe and the routing.

The developer chose real host allowlists for this stack, so coding-agent specs (`5/12` onwards) will declare their hosts. This node delivers the mechanism and proves it on a generic tool, so each agent node only adds its list.

## Proposed Changes

### What's Changing

- `ToolDef.network { egress: none | any | { hosts: [pattern] }, oauth_inbound: bool }`.
  - A pattern is `host[:port]` or `*.domain[:port]`. The port defaults to 443.
  - The section is optional, and absence means `none`. A tool that needs the network must say so; this is a declared default, not a fallback.
- `tddy_sandbox::NetworkSpec` gains `egress: EgressPolicy`, lowered from the union. The shim starts only when the policy is not `none`.
- The runner's egress shim checks the policy on every `CONNECT`.
- `run_host_relay`'s `TunnelOpen` handling also checks it.
- Each refusal is logged with the session id, tool set and host, and is visible in the session's sandbox log.
- `oauth_inbound` lowers to `NetworkSpec.allow_oauth_inbound`, replacing the recipe-derived value, but only for jails whose plan comes from tools. Agent recipes keep theirs until their own node.

### What's Staying the Same

- Every existing coding-agent jail keeps `any` until its node declares hosts, because their plans are not yet built from specs.
- The CONNECT tunnel stays end-to-end TLS. Nothing is decrypted.

## Acceptance Criteria

- [ ] Inside a jail whose only tool declares `hosts: [127.0.0.1:<port>]`, a `curl` through the shim reaches that host, and a second local listener on another port is refused with `403` naming it.
- [ ] A `TunnelOpen` sent straight to the host relay for an undeclared host is refused, even when the shim is bypassed.
- [ ] A jail whose tools all declare `egress: none` starts no shim and sets no proxy env.
- [ ] When one tool declares `any`, the jail's policy is `any`.
- [ ] A refused host appears in the session's sandbox log with the tool set that lacked it.
- [ ] A malformed pattern is refused at registry load, naming the tool.

## Out of Scope

- Agent host lists (`5/12`, `7/12`, `8/12`).
- Path- or method-level filtering.
- DNS-based IP pinning.
