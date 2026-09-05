# 2026-03-08 — Schema Path Working Directory

**Type:** Fix

Agent working_dir set to plan_dir (not parent) for plan, acceptance-tests, red, green so `schemas/xxx.schema.json` resolves to `{plan-dir}/schemas/`. Plan prompt: project discovery uses parent dir (../Cargo.toml) for project files. (tddy-core)
