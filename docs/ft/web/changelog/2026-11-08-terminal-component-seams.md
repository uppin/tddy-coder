# 2026-11-08 — Terminal component structure

No change to what the terminal does. The browser terminal's two large components are split into
focused hooks so each unit of input, scrolling and history handling can be changed and tested on its
own.
