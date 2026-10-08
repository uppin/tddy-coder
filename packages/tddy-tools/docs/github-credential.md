# The PR tools' GitHub credential

`github_create_pull_request`, `github_update_pull_request` and the other GitHub-reaching tools
authenticate as **the account the session's project is assigned**. The token is **asked for, never
delivered**: it is in no environment variable and no file, on any path.

Code: `src/github_credential.rs`; the callers are in `src/server.rs`.

## How a call gets its token

1. The tool sends a `github-token` request over the session's own toolcall socket (`TDDY_SOCKET`) when it
   runs ([toolcall verb](../../tddy-toolcall/docs/architecture.md#the-github-token-verb)). The request
   names no session, project or account; the socket is the identity.
2. The session's host resolves the project's account and answers that one call
   ([session identity](../../tddy-session-lifecycle/docs/session-identity.md)). For a `tddy-coder` tool
   session the coder relays it to the daemon
   ([host-session wiring](../../tddy-coder/docs/host-session-wiring.md)).
3. `github_api_for(repo, socket)` builds a `RealGithubPrApi::with_token` client from the answer.

The tools are synchronous and run on the MCP server's runtime, so the request runs on a thread of its
own with a runtime of its own rather than blocking inside the caller's.

## Refusals

A refusal — not assigned, unknown on this host, ambiguous, unusable, a locked or unavailable vault, no
such session — arrives as the host's own words and is returned to the agent as `{"error": <message>}`,
not as a generic authentication failure. With no `TDDY_SOCKET` the error names it. `GITHUB_TOKEN` and
`GH_TOKEN` are never consulted: the absence of a host to ask is an error, not a cue to look elsewhere.

## Tests

`github_credential` unit tests (a fixed-answer host over a real socket: the token is returned, a refusal
arrives verbatim, no socket is an error naming it, `GITHUB_TOKEN` in the environment changes nothing) and
the PR-tool tests in `server.rs` that drive a refusing host.
