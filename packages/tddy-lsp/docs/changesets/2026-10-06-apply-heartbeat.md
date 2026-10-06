# 2026-10-06 — The fake language server can be busy for ever

**Type:** Test support

`tests/bin/fake_lsp.rs` gains three modes, off unless asked for, so every other mode stays byte-identical:
`--never-quiescent` (reports `experimental/serverStatus {health: ok, quiescent: false}` with the handshake,
sends one build-script progress line with no percentage, and nothing more — no `end`, no `quiescent: true`),
`--goes-busy-after-hovers N` (a ready graph that goes busy after the Nth hover) and `--hover-never-answers`
(swallows `textDocument/hover`, answers every other request). They let a consumer wait on a server that
never gets ready without a real rust-analyzer. The double is `tests/bin` only — not shipped.

Cross-package entry: [docs/dev/changesets](../../../../docs/dev/changesets/2026-10-06-apply-heartbeat.md).
