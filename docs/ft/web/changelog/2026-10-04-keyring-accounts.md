# 2026-10-04 — Accounts screen

`#keyring` 4/9 — PR [#511](https://github.com/uppin/tddy-coder/pull/511).

A new **Accounts** screen (`#/accounts`, in the menu after **Hosts**) lists the accounts the selected
daemon keeps for you in your credential vault, grouped by provider, with each account's label and the
provider's own identifier. You can rename an account — its identity never changes — or remove one
after a confirmation. No secret ever reaches the page. The screen tells apart a vault that holds
nothing, no vault yet (choose a passphrase to create one), a locked vault (unlock it with your
passphrase; nothing is lost) and a vault that could not be read. See
[Accounts screen](../accounts-screen.md).
