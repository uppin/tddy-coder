# tddy-github

Everything that talks to GitHub: the OAuth provider and session tokens, and the REST client for
pull requests.

## Module layout

### Identity

| Module | Owns |
|---|---|
| `provider.rs`, `real.rs`, `stub.rs` | the `GitHubOAuthProvider` trait, its live implementation and the test stub |
| `auth_service.rs` | the gRPC `AuthService` implementation over `tddy_service::proto::auth` |
| `session_token_v2.rs` | the `v2` session-token format — Ed25519 over a payload naming its signer's key id: `SessionTokenSigner`, `SessionTokenVerifier`, `SessionTokenAuthority`, `KeyId`, `SessionClaims`, token kinds and TTLs. See [docs/session-token.md](docs/session-token.md) |
| `token_store.rs` | `GitHubTokenStore` |

### Pull requests

| Module | Owns |
|---|---|
| `github_rest_common.rs` | shared REST constants (`Accept`, API version, user agents) and token resolution from the environment — a pure leaf |
| `github_pr.rs` | the PR helpers `tddy-tools` exposes over MCP |
| `pr_api.rs` | the `GithubPrApi` / `GithubPrInsightApi` traits and their DTOs — `PrState`, `PrDetail`, `PrFile`, `PrReview`, `CheckRun`, `PrLookupOutcome`, … — plus the live REST implementation |

These three arrived from `tddy-workflow-recipes`, where 2,124 lines of GitHub REST sat inside a
crate about workflow recipes while the crate named after the service had no PR surface at all.
`pr_api` was `orchestrate_pr_stack/github.rs`; it is renamed because `orchestrate_pr_stack` is a
workflow-recipes concept with no meaning here.

`tddy-workflow-recipes` keeps one-line `pub use` facades at all three old paths, so
`tddy_workflow_recipes::github_pr::…` and `crate::orchestrate_pr_stack::github::…` still resolve and
no caller was edited. Write new code against `tddy_github` directly.

## Dependency rules

`tddy-github` depends on `tddy-rpc`, `tddy-service` (for the generated `proto::auth` types) and
`tddy-core`.

**The `tddy-core` edge exists for one reason**: `pr_api`'s public trait methods return
`Result<_, tddy_core::WorkflowError>`, in 16 production signatures. It closes no cycle — `tddy-core`
depends on none of `tddy-github`, `tddy-service` or `tddy-workflow-recipes`.

**`tddy-github` must never depend on `tddy-workflow-recipes`.** That crate holds the facades pointing
here, so an edge back would make the pair mutually dependent.
`tests/git_plumbing_shape.rs` asserts this.

> The same facade edge already forced one change elsewhere: `tddy-service` depended on
> `tddy-workflow-recipes`, which with `recipes → github → service` became a package cycle. Since
> `tddy-service`'s only use of recipes is inside a `#[cfg(test)]` module, that dependency is now a
> **dev-dependency** — Cargo permits cycles through those.
