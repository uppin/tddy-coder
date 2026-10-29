# 2026-10-29 — The project-home agent sees what a terminal highlights, and reads part of it

`ReadSessionTerminal` takes two optional arguments. `styles: true` adds `styles` (per row, the runs
whose colours or attributes differ from the default: `fg`, `bg`, `bold`, `dim`, `italic`,
`underline`, `inverse`, `strikethrough`, `blink`) and `highlighted_rows`, the rows painted as an
inverse or one-background bar, which is how many TUIs mark the current item. `region {row, col, rows,
cols}` reads only one pane, menu or dialog, with styles clipped and re-based to the box and the
cursor relative to it. Without either, the answer is unchanged
([#785](https://github.com/uppin/tddy-coder/pull/785)). See
[Project concept § Reading a terminal](../project-concept.md#reading-a-terminal).
