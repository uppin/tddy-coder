# 2026-04-06 — GitHub PR MCP tools (tddy-tools) and recipe prompts

- **tddy-tools**: MCP tools **`github_create_pull_request`** and **`github_update_pull_request`** (GitHub REST via **`curl`**); **`ServerInfo`** instructions name those tools when **`GITHUB_TOKEN`** or **`GH_TOKEN`** is set; mock-recorded request tests for JSON bodies and headers.
- **tddy-workflow-recipes**: **`github_rest_common`** holds shared **`Accept`**, **`X-GitHub-Api-Version`**, token resolution, and User-Agent strings for merge-pr curl and **tddy-tools**; **tdd-small** merged **`red`** prompt includes the GitHub PR tools section only with a non-empty token; merge-pr hooks continue to append GitHub PR tool awareness under the same condition.
- **Schema**: **`changeset-workflow`** accepts optional **`github_pr_tools_metadata`** alongside **`workflow`** fields.
- **Docs**: [github-pr-tools-mcp.md](../github-pr-tools-mcp.md); [workflow-recipes.md](../workflow-recipes.md); [workflow-json-schemas.md](../workflow-json-schemas.md); **`packages/tddy-tools/docs/json-schema.md`**; package **`changesets.md`** for **tddy-tools** and **tddy-workflow-recipes**; **[docs/dev/changesets/](../../../dev/changesets/)**.
