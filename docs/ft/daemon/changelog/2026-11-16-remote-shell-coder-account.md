# 2026-11-16 — Connect a Coder deployment as an account

A signed-in person connects a Coder deployment on the Accounts screen by entering its URL and
a session token. The daemon asks the deployment who the token belongs to before storing
anything, seals the pair in the person's credential vault as a `coder` account, and lists it
beside their GitHub and provider accounts. Connecting the same deployment and user again
replaces the token and keeps the account id and label. No secret reaches the screen.

See [Provider accounts § Coder accounts](../provider-accounts.md#coder-accounts) and
[Accounts screen § Connect Coder](../../web/accounts-screen.md#connect-coder).
