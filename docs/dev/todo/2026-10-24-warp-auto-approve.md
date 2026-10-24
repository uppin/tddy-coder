# 2026-10-24 — No `--auto-approve` option for Warp sessions

**Category:** Deferred from `#warp-agent` 1/2 (#717)
**Source:** the `warp-cli` session changeset (#717) § Boundaries

`warp_argv` ignores `CliLaunch`'s permission fields, and the create-session form offers Warp no
permission control, so every Warp session asks before it acts.

**Why deferred:** out of the node's scope, and the flag is one of the unconfirmed Warp facts
(`2026-10-24-warp-cli-facts-unconfirmed.md`).

**What would close it:** map `dangerously_skip_permissions` to `--auto-approve` in `warp_argv`, offer
the toggle for `warp-cli` in `CreateSessionPermissionFields`, and pin both in
`warp_cli_acceptance.rs` and a Cypress spec.
