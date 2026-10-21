# 2026-10-21 — The desktop sign-in dialog pins the fingerprint the prompt advertises

**Category:** Deferred from `#host-desktops` 8/8 (#660)
**Source:** `#host-desktops` 8/8 ([#660](https://github.com/uppin/tddy-coder/pull/660)), `/validate-prod-ready`

- `packages/tddy-web/src/components/hosts/HostDesktopSignInDialog.tsx` checks key continuity with
  `checkHostKey(hostId, question.hostPublicKeyFingerprint)`: it pins and shows the fingerprint
  **string the prompt advertises**, not one derived from `question.hostPublicKey`. A peer that
  substitutes the key can advertise the pinned string beside it, so the pin does not bind the key
  that the credential is encrypted under. `HostAddKeyAction` / `useHostPromptAnswer` already make the
  stronger check (`verifyHostKey`: derive, compare, then pin the derived value).
- The desktop **password** dialog had the same weakness before this PR (`HostDesktopOverlay` called
  `checkHostKey` on the advertised string). #660 carried it over, and did not introduce it.
- **Why deferred:** the approved specs `HostDesktopSignInAcceptance.cy.tsx` and
  `HostDesktopConnectAcceptance.cy.tsx` publish a real generated RSA key (`anRsaOaepKeypair`) beside
  a fixed fake fingerprint (`HOST_KEY_FINGERPRINT`). Under `verifyHostKey` every question would be
  `mismatched` and block the form, so the fix also has to change the fixtures of two approved specs
  to advertise the key's derived fingerprint. That is a spec change and needs review.
- **Fix shape:** have the dialog take its verdict and fingerprint from `verifyHostKey` (or reuse
  `useHostPromptAnswer`'s checked-key pairing), show the **derived** fingerprint, and have both specs
  compute `HOST_KEY_FINGERPRINT` from the generated key (`deriveHostKeyFingerprint`). Add a spec for a
  prompt whose advertised fingerprint does not match its key.
