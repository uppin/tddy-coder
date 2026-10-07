# 2026-10-07 — the CLI carries a declared `impl` retarget to `verify`

**Type:** Feature

`restructure verify --retarget OLD=NEW` (repeatable) tells `verify` which `impl` retargets the author
made, so it accounts for the differences they cause rather than reporting them. `index_client.rs::verify`
fills `VerifyRequest.retargets` from `RestructureVerifyArgs.retarget`; the flag itself is defined in
`tddy-code-restructuring`'s `RestructureVerifyArgs`.

No new subcommand and no new RPC: one field on one existing request, one flag on one existing command.

Cross-package entry: [docs/dev/changesets](../../../../docs/dev/changesets/2026-10-07-retarget-impl.md).
