# 2026-10-21 — `#host-desktops` sweep: `tddy-daemon-kernel` config split

**Type:** Refactor

`#host-desktops` 9/10 (#692). `packages/tddy-daemon-kernel/src/config.rs` becomes `config.rs` plus eight
`config/` modules, cut by the restructure engine with no hand-moved code. See the package changeset
`2026-10-21-config-split` in `packages/tddy-daemon-kernel/docs/changesets/`.

**Final measurement of the closed code issue `oversized-file-config`** (deleted with this change): production
lines (code excluding `#[cfg(test)]` items) **1,575 → 438**, under the 500-line budget. Earlier history of the
record: 1,447 at detection (2026-09-19), 1,470 / 1,472 / 1,474 / 1,476 across `#keyring`, 1,491 (#579), 1,519
(#513), 1,557 (#649), 1,575 (#659).

Seven items lost their public path (`LocalConfig`, `KeyringConfig`, `SandboxCgroupConfig`, `GitConfig`,
`AgentWarmupConfig`, `SessionRoomConfig`, `AllowedTool`); no caller outside the crate used them. Open
`misplaced-tests-privilege-drop` and `missing-tests-privilege-drop-resolve-pty-os-user` records were not touched.
