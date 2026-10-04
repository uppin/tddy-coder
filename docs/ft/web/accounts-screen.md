# Accounts screen

The **Accounts** screen at `#/accounts` shows the accounts a daemon keeps for the signed-in person —
GitHub today, other providers as they are added — grouped by provider. From it a person can
**rename** an account or **remove** one. It is reached from the hamburger menu, between **Hosts** and
**VMs**, and by direct URL.

## Why it exists

A daemon keeps linked credentials in the person's encrypted **credential vault**
([session auth § GitHub access-token retention](../daemon/session-auth.md)). Without this screen
nobody could see what is in it: an assignment of an account to a project would have nothing to choose
from, a revoked account could never be removed, and a vault that is locked or not created yet would
surface only as git operations failing later, unexplained.

## What a row shows

Per account: the **label** a person chose, the provider's own identifier for the account (the
**subject**, e.g. a GitHub login), and a note when no usable credential is stored.

**No secret ever reaches the screen.** The service has no field that could carry one and no method
that returns one; whether a credential is present is a single yes/no.

## Four outcomes, never collapsed

| The daemon's answer | What the screen says |
|---|---|
| the vault is open and holds nothing | no accounts are linked yet, and that an account appears once linked from its provider |
| no vault exists yet | you have no credential vault on this daemon yet; choosing a passphrase creates one, in the prompt the dashboard shows over every screen |
| a vault exists and is not unlocked on this daemon | your vault is locked; unlock it with your passphrase — nothing in it is lost |
| the vault could not be read | the reason, as the daemon gave it |

These are different facts with different remedies, and showing a locked or missing vault as an empty
list would lead a person to re-link accounts they already have. The screen asks for no passphrase
itself: the app-wide vault prompt does, and reloading the page brings it back after "Not now".

## Actions

- **Rename** changes only the label. The account's identity never changes, so anything that refers
  to the account — such as a project's assigned account — survives a rename. Renaming an account
  that is not linked is refused as not found.
- **Remove** forgets one account's credential, after a confirmation. The rest stay.
- A failed rename or removal is reported beside the list, which stays as it was.

**Linking a new account is not done here.** Each provider's link flow is its own; this screen shows
and curates what exists.

## Scope and access

- The list is the signed-in person's own vault on the **selected daemon**; it is refused, not shown
  empty, when the session token names no signed-in session.
- The screen is available on every connection — it is plain daemon RPC, not media or presence, so it
  is not [capability-gated](../../../packages/tddy-web/docs/capability-gating.md).
- A daemon without `auth_storage` keeps no credentials and does not offer the service.
- On a daemon whose GitHub provider is a **stub** (demo), a stub login retains no credential, so no
  vault is created by signing in and the screen reports that none exists yet.

## Related

- [App shell § Navigation menu](app-shell.md#navigation-menu)
- [Cross-daemon session authentication](../daemon/session-auth.md)
- Implementation: [tddy-web accounts screen](../../../packages/tddy-web/docs/accounts-screen.md),
  [tddy-accounts service](../../../packages/tddy-accounts/docs/accounts-service.md)
