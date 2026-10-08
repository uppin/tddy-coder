# GitHub pull request tools (tddy-tools MCP)

**Product area:** Coder  
**Updated:** 2026-04-06

## Summary

**`tddy-tools --mcp`** registers optional MCP tools that call the GitHub REST API to create or update pull requests. Tool registration and **`ServerInfo`** instructions reference the same stable names: **`github_create_pull_request`** and **`github_update_pull_request`**. Live HTTP calls use **`curl`** against **`https://api.github.com`** (the API root is a value on the client) with Bearer authentication and GitHub REST version headers; unit tests use a mock transport that records requests without network I/O.

## Authentication

- The tools authenticate as **the account the session's project is assigned** ([per-project GitHub identity](../daemon/github-identity.md)). At each call the tool asks the session's host for that account's token over the session's own toolcall socket (**`TDDY_SOCKET`**); the token is in no environment variable and no file.
- **`GITHUB_TOKEN`** and **`GH_TOKEN`** are never read. With no assigned account, an account the host does not hold, a locked vault, or no **`TDDY_SOCKET`**, the tool returns **`{"error": <reason>}`** carrying the host's own words, not a generic authentication failure.
- **`get_info`** states that the PR tools authenticate as the project's assigned account.

## REST contract

Shared constants live in **`tddy_github::github_rest_common`** (`GITHUB_ACCEPT`, `GITHUB_API_VERSION`); every REST entry point takes the token as a parameter ([REST token](../../../packages/tddy-github/docs/rest-token.md)). **tddy-tools** re-exports the pieces used by GitHub PR helpers; merge-pr workflow curl calls use the same **`Accept`**, **`X-GitHub-Api-Version`**, and User-Agent values for consistency across the codebase.

## Workflow recipe behavior

- **merge-pr:** System prompts for **`finalize`** include GitHub PR tool awareness when the workflow context carries **`github_pr_tools_available = true`**.
- **tdd-small:** The merged **`red`** system prompt includes a **GitHub PR tools** section (with guidance to prefer MCP PR tools over ad-hoc scripts) only when that flag is true.
- The flag is set by the process that drives the workflow, exactly when its host can answer token requests (a **`tddy-coder`** tool session given **`--host-session-socket`** and its session id). It is never a probe of the environment.

## Changeset workflow metadata

The **`changeset-workflow`** JSON Schema allows optional **`github_pr_tools_metadata`** for routing or tooling hints alongside **`workflow`** fields. **`tddy-tools persist-changeset-workflow`** validates payloads with that optional block.

## Related

- [Per-project GitHub identity](../daemon/github-identity.md) — which account, and what happens when there is none  
- [Workflow recipes](workflow-recipes.md) — **`MergePrRecipe`**, **`TddSmallRecipe`**, hook behavior  
- [Workflow JSON Schemas](workflow-json-schemas.md) — **`changeset-workflow`**, schema registry  
- **`packages/tddy-workflow-recipes/docs/json-schema.md`** — the schema library, and the CLI and MCP transport notes  
- **`packages/tddy-workflow-recipes/docs/workflow-schemas.md`** — **`goals.json`** and generated schemas  
