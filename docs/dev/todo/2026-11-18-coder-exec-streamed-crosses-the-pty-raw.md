# 2026-11-18 — A streamed Coder command still relays its pipes through the workspace's pty

**Category:** Known defect
**Source:** the Coder `remote-dev` smoke test of 2026-11-18 (orchestrator session `01a1ecee-e942-7982-abfc-104c6c4baa98`, diagnostics session `01a1ecfc-0fac-77a0-bb0d-ac20a90593ae`), left open by the fix that made Coder one-shots cross the pty intact ([`2026-11-18-coder-one-shots-cross-the-pty-intact`](../changesets/2026-11-18-coder-one-shots-cross-the-pty-intact.md))

`CoderShell::exec_streamed` hands the caller `coder ssh`'s own pipes. The workspace side is a pty,
so closing stdin never reaches the command as an EOF. Output is rewritten by `onlcr`, and input
passes through canonical mode, where control bytes are acted on. `tddy-worktree-service` streams git
pack verbs this way, and a pack is binary, so a Coder target's remote git verbs are exposed. One-shots
(`exec`, `read_file`, `write_file`) are pty-safe.

**Why deferred:** a byte-exact duplex stream needs more than the one-shot wrapper's counted base64
lines: framing both directions, or a transport with no pty (OpenSSH through `coder config-ssh` /
`coder ssh --stdio` as a proxy command). That is its own design.

**What would close it:** a stand-in `coder` whose streamed command never sees EOF and translates
newlines, failing `contract::a_streamed_command_relays_until_it_exits` and a binary round-trip,
then a transport that passes both against `mantasi/remote-dev`.
