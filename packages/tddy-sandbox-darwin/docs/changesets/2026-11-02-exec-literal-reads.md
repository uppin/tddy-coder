# 2026-11-02 — `process-exec*` for exec literal reads

`#agent-tools` 1/12 ([#790](https://github.com/uppin/tddy-coder/pull/790)). `render_plan` now emits an exec-marked literal read into `process-exec*`, not only exec subpaths, so a declared tool's binary runs without its directory being executable. Agent jails declare only exec subpaths; their profiles are unchanged.
