# 2026-09-10 — `packages/tddy-web/src/buildId.ts` is a committed build artifact

**Category:** Future enhancement
**Source:** `#unbundle` node 4, [#473](https://github.com/uppin/tddy-coder/pull/473)

`packages/tddy-web/src/buildId.ts` regenerates on every build and is committed, so it appears as
diff noise in every PR that ran a web build — and in a stack, it is a **guaranteed conflict on every
cascade rebase**, in a file whose content nobody is reviewing.

Either generate it into an ignored path at build time (the way the other generated web output is
handled), or stop committing it and let the bundler inject the value.
