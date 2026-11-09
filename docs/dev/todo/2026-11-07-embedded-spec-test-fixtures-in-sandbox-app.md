# 2026-11-07 — `embedded_spec` has no production caller; tddy-sandbox-app's test fixtures still use it

**Category:** Deferred from `#agent-tools 25/28` ([#833](https://github.com/uppin/tddy-coder/pull/833)), for the sweep node
**Source:** `packages/tddy-coding-agent-specs/src/lib.rs` (`embedded_spec`)

#833 moved the split session onto the host's registry, which left `tddy_coding_agent_specs::embedded_spec`
with no production caller in the workspace. Two `#[cfg(test)]` fixtures in `tddy-sandbox-app` still
build their agent spec through it, and were not changed there:

- `packages/tddy-sandbox-app/src/host_agent.rs` — `a_claude_agent()` (`embedded_spec("claude")`)
- `packages/tddy-sandbox-app/src/main.rs` — `the_spec_of(id)` (`embedded_spec(id)`)

`tddy-agent-launch` also calls it from two `#[cfg(test)]` helpers (`start_request_checks.rs`,
`harness_from_spec.rs`).

What would close it: the sweep node decides whether `embedded_spec` stays as a public test-support
constructor or is replaced by `CodingAgentRegistry::builtin()` in those four fixtures, and deletes it
if so.
