# 2026-06-21 — Demo goal Phase 2: QEMU SSH impl, daemon VM lifecycle, web UI

- `QemuDemoVm` fully implemented: `boot` (spawns QEMU, polls SSH up to 5 min), `deploy` (SSH each step), `verify` (captures output), `forward` (TCP validate + `http://localhost:<port>` URL), `shutdown` (monitor `system_powerdown`)
- Daemon owns VM lifecycle: `StartDemoVm`/`StopDemoVm`/`GetDemoVmStatus` RPCs; `DemoVmHandle` tracks per-session state (`Booting`, `Running { share_url }`, `Error`)
- Web UI `DemoVmControls` component: Launch/Stop/Retry buttons, booting badge, "Open demo" share URL; shown for `workflowGoal === "demo"` sessions in `ConnectionScreen`
- Demo system prompt updated with 7-step QEMU deploy flow: build qcow2, wait for VM running, SSH deploy steps, verify, confirm port-forward, report `share_url`, post to Telegram, submit
