# 2026-11-25 — The account a token belongs to, over `GET /user`

**Type:** Fix · PR [#1044](https://github.com/uppin/tddy-coder/pull/1044)
Cross-package entry: [`docs/dev/changesets/2026-11-25-accounts-identity-decoupled.md`](../../../../docs/dev/changesets/2026-11-25-accounts-identity-decoupled.md)

New `user_api` module: `parse_authenticated_user(body)` (pure — id and login required,
`avatar_url` and `name` best-effort) and `fetch_authenticated_user(api_base, token)`
over the curl transport's absolute-path GET, sync like every other REST call here. A
blank token is refused before curl runs; any other failure is a plain sentence for the
derivation refusal to quote. `api_base` is a value — `https://api.github.com` in
production, a local listener in tests — never the environment.

Detail: [rest-token.md](../rest-token.md).
