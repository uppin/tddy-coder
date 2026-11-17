# 2026-11-16 — The rendered Cursor hooks file survives Cursor's comment strip

**Type:** Fix · PR [#933](https://github.com/uppin/tddy-coder/pull/933)

`render_cursor_hooks_json` escapes every `/` as `\/` in the serialized bytes: Cursor reads
`.cursor/hooks.json` through a `parseJSONC` that strips `//.*$` and `/*…*/` blind to string
literals, and an unescaped `--daemon http://…` cut each command's string and failed the whole
file — leaving the session with no hooks at all. `\/` is valid JSON parsing back to `/`, and
no `/` in the output stands outside a string, so the replace cannot touch the syntax. The
byte-exact render test pins the escaped bytes and the seven installed events, and a
regression test applies Cursor's two strips to the rendered file and requires it to parse
with every event's command intact.
