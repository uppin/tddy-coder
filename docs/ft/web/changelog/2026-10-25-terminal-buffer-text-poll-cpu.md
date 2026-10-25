# 2026-10-25 — Terminals no longer keep the app busy while idle

An open session terminal — and above all one with earlier history loaded — no longer holds the
dashboard (and Tddy Desktop's window process) at a full CPU core while nothing is printing. Terminal
text is re-read only when the terminal has actually changed.
