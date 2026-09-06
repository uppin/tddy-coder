# 2026-09-06 — `packages/tddy-web/src/gen/daemon_config_pb.ts` was regenerated without `buf`

**Category:** Deferred from `optional-livekit` (#449)
**Source:** optional-livekit common-room switch, #449

No npm registry
was reachable in that worktree, so the descriptor was rebuilt with `protoc` plus a `json_name`
strip that reproduces `protoc-gen-es` byte-for-byte (verified against the committed file *before*
editing), and the two interface fields were written by hand. Re-run `bun run generate` once `buf`
is available and confirm the file is unchanged.
