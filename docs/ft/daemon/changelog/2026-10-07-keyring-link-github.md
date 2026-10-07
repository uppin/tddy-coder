# 2026-10-07 — Add a second GitHub account without signing out

- **Link another GitHub account to your vault** while staying signed in as you are. Linking never
  signs anyone in or out; your session and identity are unchanged.
- **One person is one account.** Accounts are recognised by GitHub's user id, not the login name, so
  linking again refreshes the credential and keeps the account's label and any project assigned to it.
- **The account your session rests on cannot be removed**, because the vault's key derives from it;
  the refusal says why. Every other account can be removed.
- **Clear outcomes**: pending, linked, denied, expired, and vault locked (never reported as a
  refusal). The first check waits the interval GitHub names, and the daemon holds early checks back.
- On a daemon with a stub GitHub provider, linking is refused rather than pretended.

See [Account linking](../account-linking.md).
