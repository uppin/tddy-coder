# GitHub pull request tools (tddy-tools MCP)

**Product area:** Coder  
**Updated:** 2026-04-06

## Summary

**`tddy-tools --mcp`** registers optional MCP tools that call the GitHub REST API to create or update pull requests. Tool registration and **`ServerInfo`** instructions reference the same stable names: **`github_create_pull_request`** and **`github_update_pull_request`**. Live HTTP calls use **`curl`** against **`https://api.github.com`** with Bearer authentication and GitHub REST version headers; unit tests use a mock transport that records requests without network I/O.

## Authentication

- **`GITHUB_TOKEN`** (preferred) or **`GH_TOKEN`** must be non-empty for successful REST calls.
- When neither variable is set, MCP tool handlers return a clear authentication error; **`get_info`** describes GitHub PR tools only when a token is present so agents see a single coherent contract.

## REST contract

Shared constants and token resolution live in **`tddy-workflow-recipes::github_rest_common`** (`GITHUB_ACCEPT`, `GITHUB_API_VERSION`, **`github_token_from_env`**, **`github_env_token_present`**). **tddy-tools** re-exports the pieces used by GitHub PR helpers; merge-pr workflow curl calls use the same **`Accept`**, **`X-GitHub-Api-Version`**, and User-Agent values for consistency across the codebase.

## Workflow recipe behavior

- **merge-pr:** System prompts for **`finalize`** include GitHub PR tool awareness when **`github_env_token_present()`** is true (same environment check as REST merge paths).
- **tdd-small:** The merged **`red`** system prompt includes a **GitHub PR tools** section (with guidance to prefer MCP PR tools over ad-hoc scripts) only when **`github_env_token_present()`** is true.

## Changeset workflow metadata

The **`changeset-workflow`** JSON Schema allows optional **`github_pr_tools_metadata`** for routing or tooling hints alongside **`workflow`** fields. **`tddy-tools persist-changeset-workflow`** validates payloads with that optional block.

## Related

- [Workflow recipes](workflow-recipes.md) — **`MergePrRecipe`**, **`TddSmallRecipe`**, hook behavior  
- [Workflow JSON Schemas](workflow-json-schemas.md) — **`changeset-workflow`**, schema registry  
- **`packages/tddy-tools/docs/json-schema.md`** — CLI and MCP transport notes  
- **`packages/tddy-workflow-recipes/docs/workflow-schemas.md`** — **`goals.json`** and generated schemas  
