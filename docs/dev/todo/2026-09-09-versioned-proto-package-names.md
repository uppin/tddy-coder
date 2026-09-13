# 2026-09-09 — The protos have no versioned package names, and the split was the cheap moment to add them

**Category:** Future enhancement
**Source:** `#unbundle` node 1, [#470](https://github.com/uppin/tddy-coder/pull/470)

`connection.proto` declares a bare `package connection;`, and the two protos split out of it in #470
followed suit — `package host;` and `package worktree;`. Nothing in the workspace uses a versioned
package name (`tddy.host.v1`), so nothing can be evolved behind one.

Cutting a service into three was the one cheap opportunity to introduce them: every consumer's import
paths were being rewritten anyway, so the rename would have ridden along for free. It was
**deliberately declined** — it would have put a second, unrelated breaking change into every node's
web migration, and the `#unbundle` stack has seven more nodes that each migrate `tddy-web`. A
mechanical move that is also a naming change is not reviewable as either.

The cost of deferring is that the next attempt pays the full price: renaming a package renames every
generated symbol in Rust and TypeScript with no move to hide behind. Worth doing as its own change,
across all protos at once, after the `#unbundle` stack lands — a partial rename is worse than none,
because it makes the convention unreadable.
