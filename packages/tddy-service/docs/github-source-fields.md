# GitHub source on the wire

A project's sessions take their GitHub identity from a vault account or from the host's own `gh`
login ([per-project GitHub identity](../../../docs/ft/daemon/github-identity.md)). The wire carries
that choice in existing messages; no RPC exists for it, so `RESIDUAL_METHODS` is unaffected.

| Message | Field | Meaning |
|---|---|---|
| `ProjectEntry` (`project.proto`) | `local_gh = 9` | the project uses Local gh |
| `SetProjectAccountsRequest` (`project.proto`) | `local_gh = 5` | set with the accounts in one replace; refused with `InvalidArgument` beside a GitHub assignment |
| `StartSessionRequest` (`session.proto`) | `github_source = 40` | the source the target host must use; filled by a forwarding host from its own row |
| `GetHostToolingRequest` (`host.proto`) | `github_hostname = 3` | the GitHub host the `gh` probe is about; empty probes every host |

`enum GithubSource` (`types.proto`) is `GITHUB_SOURCE_UNSPECIFIED = 0`, `GITHUB_SOURCE_VAULT = 1`,
`GITHUB_SOURCE_LOCAL_GH = 2`. Unspecified means the receiving host reads its own project row.

Behaviour: [session identity](../../tddy-session-lifecycle/docs/session-identity.md#which-source-a-session-uses),
[project storage](../../tddy-projects/docs/project-service.md#local-gh).
