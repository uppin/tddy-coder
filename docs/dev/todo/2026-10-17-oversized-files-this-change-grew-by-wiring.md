# 2026-10-17 — Oversized files this change grew by a few lines of wiring

**Category:** Deferred refactor
**Source:** the clone passphrase prompt (2026-10-17) — `/pr-wrap` step 3.5, file-length gate

The clone passphrase prompt touched five files that were already over the 500-production-line budget and
grew each by the minimum wiring it needed:

| File | Production lines | Cause |
|---|---|---|
| `packages/tddy-daemon/src/runtime.rs` | 1781 → 1794 | starts the one `AskpassEndpoint` and installs it on the host |
| `packages/tddy-web/src/components/sessions/CreateSessionPane.tsx` | 785 → 795 | one import and one `<StartPassphrasePrompt>` mount |
| `packages/tddy-service/build.rs` | 723 → 729 | a prost pass for `askpass.proto` |
| `packages/tddy-daemon-rpc/src/project/coordinate_handlers.rs` | 658 → 660 | `env` on two `CloneRequest` constructions |
| `packages/tddy-session-lifecycle/src/connection_service.rs` | +4 (by diff) | the `askpass` field and `mod svc_askpass_wiring;` |

**Why deferred, with the developer's consent:** none of the growth is logic. The feature's own code is in new
modules (`askpass/`, `clone_env.rs`, `clone_planner.rs`, `clone_backend.rs`, `clone_ssh_env.rs`,
`start_keepalive.rs`, `stream_relay.rs`, `useHostPromptAnswer.ts`, `StartPassphrasePrompt.tsx`), and a
decomposition of a 1,794-line runtime inside a feature PR would bury a reviewable diff under a mechanical one.
Each file already has a code-issue record (`oversized-file-*.md`, all unclaimed) that now carries the
measurement. Whoever closes those records should look at the askpass wiring in `runtime.rs` as a candidate
for the seam that moves out with the host-service construction.
