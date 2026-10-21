# 2026-10-21 — `HostPromptEvent.offered_auth_methods` is never populated

**Category:** Deferred from `#host-desktops` 8/8 (#660)
**Source:** `#host-desktops` 8/8 ([#660](https://github.com/uppin/tddy-coder/pull/660)), `/validate-prod-ready`

- `host.proto` `HostPromptEvent.offered_auth_methods = 8` was added by the #660 contract, so that a
  `DESKTOP_CREDENTIALS` prompt could say which methods the form should offer. Green took the methods
  from `InspectHostDesktop` instead, so `host_prompt_stream.rs` always sends `Vec::new()`. The browser
  never reads the field. The Cypress prompt feed (`cypress/support/rpc/hostPromptFeed.ts`) and both
  desktop sign-in specs still set it on the frames they raise.
- **Why deferred:** removing it means a proto change (with `reserved 8;`), regenerated TS in two
  `gen/` directories, and edits to the fixtures of two approved specs. The field does no harm on the
  wire today, so this was not worth changing those specs late in the stack.
- **Fix shape:** delete the field and reserve its number. Drop `offeredAuthMethods` from
  `hostPromptFeed.ts` and from the frames in `HostDesktopSignInAcceptance.cy.tsx` and
  `HostDesktopConnectAcceptance.cy.tsx`. Then remove the `Vec::new()` from `host_prompt_stream.rs`.
