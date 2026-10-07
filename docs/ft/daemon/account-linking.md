# Account linking

A person can add a **second GitHub account** to their credential vault without leaving the one they
are signed in with. An account is a record in the vault; exactly one of the records is the one the
session was established with, and linking adds a record without touching that.

## What a person can do

- Start **Add account** from the Accounts screen ([Accounts screen](../web/accounts-screen.md)) at a
  provider the vault already holds an account at.
- Be shown a **code** and the **address to enter it at**, approve the account at the provider, and see
  it appear in the list.
- Link as many accounts as they like. Each is assignable and propagates like any other record.

## Linking never signs anyone in

| | Login | Link |
|---|---|---|
| Needs an existing session | no | **yes** — the vault must be open |
| Produces a session token | yes | **no** |
| Changes who the caller is | yes | **no** |
| Result | a session, and a record | a record, and nothing else |

No response of the link flow carries a token of any kind. After a link the session token and the
signed-in identity are exactly what they were. Login, refresh and logout are unchanged.

## One person is one account

An account is recognised by the provider's own **user id**, never by the login name, which can be
changed and then registered by someone else.

- Linking an account already in the vault is a **re-link**: the credential is replaced, and the
  account keeps its identity and the label a person gave it. A project assigned to that account still
  resolves to it afterwards.
- The same user id under a changed login name is the same account, shown under the new name.
- Linking the account the session was established with updates that record in place.

A linked account asks for the same permissions as login, so it can act (push, call the provider's API)
and not only be listed.

## The session's account cannot be removed

The vault's key derives from the account the session was established with, so removing it would make
every other record unreadable. Removing it is **refused**, with that reason. Signing out is the
operation that ends a session. Every other linked account can be removed. The Accounts list marks
which account the session belongs to.

## Outcomes a person is told apart

| Outcome | Meaning |
|---|---|
| Pending | nobody has approved yet; the code stays on screen |
| Linked | the account is in the vault and appears in the list |
| Denied | the person refused at the provider |
| Expired | the code outlived its window |
| Vault locked | the approval had nowhere to go — the vault is locked or does not exist yet. This is never reported as a refusal: nothing is wrong at the provider |

An abandoned attempt does not outlive its code: it is forgotten once its window has passed, and a
finished attempt is forgotten at once.

## Polling

The provider names a minimum interval. The page waits that interval **before its first poll**, and
after each pending answer waits the interval the daemon last returned. The daemon also enforces the
interval itself: a poll that arrives early is answered pending, carrying the interval to wait,
without asking the provider. The first poll after beginning is not held back by the daemon.

## On a daemon that cannot link

Linking needs the daemon to be wired to a real provider. A daemon whose GitHub provider is a **stub**
(demo) issues synthetic tokens and wires no linking: the link requests are refused as a failed
precondition, and nothing is substituted. The Accounts screen still shows **Add account** there.

## Related

- [Cross-daemon session authentication](session-auth.md)
- [LiveKit and auth services](auth-livekit-services.md)
- [Accounts screen](../web/accounts-screen.md)
- Implementation: [tddy-accounts account linking](../../../packages/tddy-accounts/docs/account-linking.md)
