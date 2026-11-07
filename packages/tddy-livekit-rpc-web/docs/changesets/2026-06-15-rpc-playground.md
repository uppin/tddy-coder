# 2026-06-15 — **RPC Playground

**Type:** Feature

reflection proto codegen + Cypress LiveKit testkit** — `reflection_pb.ts` generated via buf (`buf.gen.yaml`); exported from `index.ts`; `ReflectionTestHarness.tsx` Cypress support; auto-start LiveKit Docker container (`livekitDockerTestkit.ts`) when `LIVEKIT_TESTKIT_WS_URL` not set; `reflection.cy.tsx` (4 tests: list_services, file descriptor fetch, server-streaming, bidi-streaming). Feature [rpc-playground.md](../../../../docs/ft/daemon/rpc-playground.md). (tddy-livekit-web)
