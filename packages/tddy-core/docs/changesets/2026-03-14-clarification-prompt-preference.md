# 2026-03-14 — Clarification Prompt Preference

**Type:** Bug Fix

BackendInvokeTask prefers `prompt` over `feature_input` when both exist. Fixes clarification loop: before_* hooks (e.g. before_acceptance_tests) set prompt with follow-up when resuming from clarification; prompt was previously ignored in favor of feature_input. (tddy-core)
