# 2026-11-16 — An orchestrator agent cannot start a session on an SSH host or a Coder workspace

**Category:** Gap (found by CI on `#remote-shell` 3/7, [#917](https://github.com/uppin/tddy-coder/pull/917))
**Source:** `packages/tddy-tool-engine/src/start_session_args.rs` (`INTERNAL_START_FIELDS`)

`StartSessionRequest.target` names the machine a session's agent runs on. The start-args parity test
requires every start field to be either offered by the orchestrator's `start_session` tool or
withheld with a reason. #917's CI fix withheld `target`, so the tool's schema does not carry it.

The web's Host field and direct RPC callers can start a session on a derived host. An orchestrator
agent, a project-home main or a Telegram start cannot: they reach `StartSession` only through the
tool.

**Why deferred:** offering the field means a tool-side way to name a target, which is a design
decision, not a fix: the tool would need the derived hosts of the daemon it starts on
(`HostService.ListDerivedHosts`), and for a Coder workspace the caller's account, which a tool call
made by an agent does not have the vault for. #917's boundary was the launch, and the fix was made
under CI to keep the parity test honest.

**What would close it:** a `target` argument on the `start_session` tool (by derived-host id, resolved
on the daemon), a `list_derived_hosts` tool beside it, a decision on whose Coder credentials an
agent-made start uses, and the parity test moving `target` from withheld to offered.
