# 2026-03-29 — GitHub stub auth from stub codes

**Type:** Feature

**`build_auth_service_entry`** in **`run.rs`** enables stub **`AuthService`** when **`--github-stub`** is true **or** **`--github-stub-codes`** contains non-whitespace after trim, so test harnesses receive stub OAuth without requiring both flags. Operators avoid passing **`--github-stub-codes`** on production-like invocations unless stub mode is intended. [docs/ft/coder/changelog/](../../../../docs/ft/coder/changelog/). (tddy-coder)
