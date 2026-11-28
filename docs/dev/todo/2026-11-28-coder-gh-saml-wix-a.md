# `gh` on Coder workspace fails wix-a with SAML enforcement (investigation)

**Status:** investigated 2026-11-28 · no code change · operator action required

## Symptom
Session `01a22204-…` (Coder target `mantasi/remote-big`, agent in tmux `tddy-01a22204-…`):
`gh` reads `wix-private/responsive-editor-packages`, but `gh pr view 6` on
`wix-a/custom-editor-playground` fails with "Resource protected by organization SAML enforcement",
even after the operator ran `gh auth refresh` and clicked the SSO links.

## Evidence (tokens redacted to last 4 chars)
| Check | Result |
|---|---|
| `gh auth status` (workspace) | one account `mantasindrasius`, active, token `gho_…dDzt`, scopes `admin:public_key, gist, read:org, repo` |
| `~/.config/gh/hosts.yml` | single user, single token `gho_…dDzt`; **mtime 2026-10-21 05:54 UTC — unchanged since** |
| Agent env (`claude`, pane pid of tmux session, + children) | **0** `GH_*`/`GITHUB_*` vars; tmux global env also 0. tddy injects no token (re-verified) |
| git credential helper | none; remote is `git@github.com:` (SSH), only `GIT_ASKPASS` = Coder's |
| `gh api -i repos/wix-a/custom-editor-playground` | `403`, `X-GitHub-SSO: required; url=…/orgs/wix-a/sso?authorization_request=…`, `X-OAuth-Client-Id: 178c6fc778ccc68e1d6a` (GitHub CLI app), `X-Accepted-OAuth-Scopes: repo` |
| GraphQL `repository(wix-a/…)` | `FORBIDDEN`, `saml_failure: true`, `ip_allow_list_failure: false` |
| `gh api -i orgs/wix-a`, `orgs/wix-a/repos` | 200 (public data only) |
| `/user/memberships/orgs` | wix-a membership `active` |
| `repos/wix-private/responsive-editor-packages` | 200 — token *is* SSO-authorized for wix-private |

## Root cause
SAML SSO authorization is **per token, per org**. The only token gh uses on the workspace —
from the agent and from a fresh login shell alike — is `gho_…dDzt`, minted 2026-10-21. It was
authorized for `wix-private` but **never for `wix-a`**. The operator's `gh auth refresh` did **not**
land on this workspace (hosts.yml was not rewritten), so it most likely ran on the Mac and the SSO
links authorized the Mac's token. Every new `authorization_request` link GitHub issues refers to
`…dDzt`; it stays unauthorized until that link is completed for wix-a *while signed in to the wix-a
IdP session* (completing an SSO login without the final "Authorize" click does not grant it).

Not the cause: stale env token in tmux (none), multiple gh accounts (one), tddy-injected token
(none), missing scope (`repo` present), IP allow list (false), OAuth-app approval (the token is a
GitHub CLI token and wix-private accepts it; SAML, not app restriction, is what is reported).

## Operator steps
1. On the workspace (not the Mac): `coder ssh mantasi/remote-big` then
   `gh auth refresh -h github.com -s repo,read:org` (or `gh auth login -h github.com --web`).
   Confirm `stat -c %y ~/.config/gh/hosts.yml` changes and the token suffix differs from `dDzt`.
2. During the device flow, on the GitHub "Authorize GitHub CLI" page click **Authorize** next to
   **wix-a** (and wix-private) under *Organization access*.
   Alternative without a new token: `gh api -i repos/wix-a/custom-editor-playground`, open the
   `X-GitHub-SSO` url, finish the IdP login and press **Authorize**.
   Or: github.com → Settings → Applications → Authorized OAuth Apps → GitHub CLI → grant wix-a.
3. Verify: `gh api -i repos/wix-a/custom-editor-playground | head -1` → `200`.
4. No agent restart needed: the agent reads hosts.yml on every `gh` call (no token in its env).

## Product follow-ups (tddy)
- None required for this bug. Optional: a doctor check for Coder targets that runs
  `gh api -i repos/<origin>` and surfaces `X-GitHub-SSO` with the instruction to authorize *on the
  workspace* — operators naturally refresh on their own machine.
- Separate observation: `ssh -T git@github.com` from a non-session shell gets `publickey` denied
  and the key needs a passphrase; git over SSH works only through the forwarded agent (#1068).
