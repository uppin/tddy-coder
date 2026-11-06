# 2026-10-22 — `tddy-sandbox-app` cannot start a jailed OpenCode or Muse Code session

**Category:** Deferred scope
**Source:** [#provider-route 9/9 (#670)](https://github.com/uppin/tddy-coder/pull/670) — sandboxed CLI
sessions on a provider account. Narrowed by `#agent-tools` 10/12
([#799](https://github.com/uppin/tddy-coder/pull/799)): the refusal is
`AgentChoiceError::RouteRequired` in `packages/tddy-sandbox-app/src/agent_spec.rs` (`choose_agent`).

## What is missing

The daemon starts and resumes `opencode-cli` and `muse-cli` sessions in a jail, each from its
tool definition. The standalone `tddy-sandbox-app --agent <id>` chooses either one from the same
registry, but refuses to start an agent whose spec sets `route.required_in_jail`:

> `--agent opencode is not started by tddy-sandbox-app yet: a jailed opencode session runs on a
> provider account, and this app passes none`

## Why it was not done in #670

A jailed OpenCode or Muse session **needs a provider account**. The daemon refuses one without
(decision (b) on #670), because the jail's `$HOME` is the session's own scratch and holds no
credentials. The daemon gets the account from the session owner's **sealed vault**, through
`SessionAccountAccess`, and hands the jail a `HarnessRoute`: secrets through `TDDY_SECRET_*` files,
the OpenCode config through the route files directory.

`tddy-sandbox-app` runs without a daemon. It has no vault session and no account picker. The binary
and the home are no longer missing. Each spec's binary override names its binary, and the spec's
`home:` section gives it a persistent home. To start them the app needs:

1. A way to name an account (`--provider-account <id>`) and to unlock the owner's vault from a
   terminal, or a daemon RPC that resolves the route for it (`daemon_client.rs` already talks to a
   daemon).
2. The route that results, passed to `lower_tool_jail` as `ToolJailInputs.route`, which the app
   leaves `None` today. The lowering side already supports this.

Item 1 is a new credential surface for a dev tool. That is not small, and it is a security decision
on its own.

## Recommendation

**A follow-up PR that resolves the route through the daemon, not a local vault.** Add a
`ResolveSandboxRoute`-shaped call (or reuse the start path with a dry-run flag) to the daemon client
the app already has. That way the key never leaves the daemon's process except into the jail's
secret files. Teaching the app to open the vault itself would duplicate the vault-unlock UX in a
second binary.

Until then the refusal stays: it is accurate, and it names what is missing.
