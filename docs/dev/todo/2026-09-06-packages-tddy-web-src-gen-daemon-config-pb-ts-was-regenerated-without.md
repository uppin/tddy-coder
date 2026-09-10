# 2026-09-06 — `packages/tddy-web/src/gen/daemon_config_pb.ts` was regenerated without `buf` — resolved 2026-09-09

**Category:** Deferred from `optional-livekit` (#449)
**Status:** Resolved
**Source:** optional-livekit common-room switch, #449

**Resolved 2026-09-09** by [#470](https://github.com/uppin/tddy-coder/pull/470). The confirmation this entry asked for is now automatic rather than a
one-off: `scripts/generated-code.sh check` regenerates every committed generated directory through
the real toolchain and fails CI on any difference, so a hand-reproduced descriptor cannot survive
undetected. It ran clean on all four directories, `daemon_config_pb.ts` among them.


No npm registry
was reachable in that worktree, so the descriptor was rebuilt with `protoc` plus a `json_name`
strip that reproduces `protoc-gen-es` byte-for-byte (verified against the committed file *before*
editing), and the two interface fields were written by hand. Re-run `bun run generate` once `buf`
is available and confirm the file is unchanged.
