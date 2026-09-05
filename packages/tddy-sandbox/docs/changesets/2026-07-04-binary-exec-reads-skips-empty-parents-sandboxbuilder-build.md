# 2026-07-04 — `binary_exec_reads` skips empty parents; `SandboxBuilder::build` drops empty-host reads

**Type:** Fix

a bare binary name (`Path::parent("claude") == Some("")`) produced a `(subpath "")` read grant, which macOS `sandbox-exec` rejects (`empty subpath pattern`, exit 65) and which — as an enclosing subpath — shadowed the entire read allow-list (every path `starts_with("")`). Both paths now filter empty hosts so an empty subpath can never reach a rendered profile. Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#281](https://github.com/uppin/tddy-coder/pull/281). (tddy-sandbox, tddy-daemon)
