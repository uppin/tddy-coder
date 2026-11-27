# 2026-11-28 — The sandbox crates go through the Fs port

The production disk I/O of `tddy-sandbox`, `tddy-sandbox-app`, `tddy-sandbox-recipes`,
`tddy-sandbox-runner`, `tddy-sandbox-cgroups` and `tddy-sandbox-darwin` goes through the
`tddy-fs` port (refactor, no behaviour change): materialize, context walks, manifests,
recipes, the cgroup broker and both binaries' session setup run behind `<fn>_on` seams and
installed handles with `DiskFs` only at composition roots. No port surface was added.
