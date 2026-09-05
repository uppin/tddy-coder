# 2026-06-21 — **Demo goal Phase 2

**Type:** Feature

demo system prompt 7-step QEMU deploy** — `tdd/demo.rs` system prompt updated: (1) read `demo-plan.md`; (2) build qcow2 via `tddy-tools build --target <build_target>`; (3) wait for VM `Running` state by polling daemon; (4) SSH deploy steps; (5) execute `verify_command` + assert success; (6) confirm port-forward active, report `share_url`, post to Telegram; (7) submit JSON with `share_url`. Test extended with assertions on `deploy_steps`, VM-wait instructions, and `share_url` field. Feature [coder/demo-goal.md](../../../../docs/ft/coder/demo-goal.md). (tddy-workflow-recipes)
