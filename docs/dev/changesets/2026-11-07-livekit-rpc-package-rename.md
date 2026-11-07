# 2026-11-07 — LiveKit RPC transport package rename (#841, `#web-split` 4/16)

- `git mv packages/tddy-livekit-web packages/tddy-livekit-rpc-web`; manifest name, Cypress JUnit name,
  codegen manifest rows, `tddy-web` dependency/aliases/imports, root workspaces and `bun.lock` updated.
- CI: `web-tddy-livekit-web.yml` → `web-tddy-livekit-rpc-web.yml`.
- Rule test `scripts/livekit-rpc-package-name.test.ts` pins the transport's identity (not a ban on the
  old name, which `feature/web-split/livekit` reuses for the wiring package).
- Kept: debug namespace `tddy:livekit-web:*`; historical records untouched.
- Backlog touched, not fixed: `2026-10-18-web-tests-start-livekit-from-the-floating-master-image.md`,
  `2026-10-18-livekit-web-start-echo-server-intermittently-exceeds-its-task-timeout.md` (paths updated).
