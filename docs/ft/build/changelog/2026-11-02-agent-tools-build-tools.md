# 2026-11-02 — Tool targets are tool definitions; hermetic actions

A `type: tool` target is an inline tool definition or a `ref:` to a registry tool, and its binary
may be another target's output, built first. The legacy `{bin_dir, commands}` shape is refused. An
action with `sandbox: true`, or every action of `tddy-tools build --hermetic`, runs jailed: inputs
read-only, outputs the only writable paths, only its tool closure executable, no network unless a
tool declares it. The cache key covers tools and hermeticity. See
[tddy-build § Hermetic actions](../tddy-build.md#hermetic-actions) for the known limits.
