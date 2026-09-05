# 2026-07-23 — **bsp-build-server

**Type:** Feature

`bsp.BspService` proto + codegen** — new `proto/bsp.proto` defines `BspService` (`WorkspaceBuildTargets`/`BuildTargetSources`/`BuildTargetOutputPaths`/`WorkspaceReload`/`BuildTarget{Compile,Test,Run}`); `build.rs` codegen pass + descriptor-set entry, `lib.rs` proto module + `BspServiceServer` re-export (reflection derives names from the mounted entries — no names-list edit). Each request message carries optional `session_token`/`session_id` for the daemon's session-addressed service (empty/ignored on the coder participant). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). Feature [bsp-build-server.md](../../../../docs/ft/coder/bsp-build-server.md). (tddy-service)
