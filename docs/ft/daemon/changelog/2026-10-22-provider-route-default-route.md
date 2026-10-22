# 2026-10-22 — A default provider account per harness

A person can mark one of their provider accounts as their **default** for Claude CLI or OpenCode CLI
sessions, from its row on the Accounts screen. A session of that type that names no account then runs
on it, and the New-session form starts with it selected; choosing "Anthropic (built in)" or another
account still wins for that one session. Each harness has one default per person: marking a second
account moves it. A harness the account cannot drive is refused with the reason. The default is kept
in the account's own vault record, so it survives a daemon restart and syncs with the account. While
the person's vault is locked, a start that names no account is refused rather than run on the vendor.
See [Provider accounts § A person's default per harness](../provider-accounts.md#a-persons-default-per-harness).
