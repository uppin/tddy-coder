# 2026-06-21 — **Demo goal Phase 2

**Type:** Feature

QemuDemoVm SSH/process impl + unit tests** — `QemuDemoVm::boot` spawns `qemu-system-x86_64` (detached) and polls SSH up to 5 min; `deploy` runs each step via `ssh root@127.0.0.1 -p <port>`, exits early on non-zero; `verify` captures stdout+stderr; `forward` validates TCP reachability on host port (1 s timeout) and returns `ForwardHandle { share_url: "http://localhost:<port>" }`; `shutdown` sends `system_powerdown` via monitor socket; `wait_for_ssh_port(host, port, timeout)` TCP-polls every 100 ms; `send_monitor_command(socket, cmd)` async Unix-socket write; `QemuVmArgs::monitor_socket_path(port)` → `/tmp/tddy-demo-monitor-{port}.sock` for per-instance sockets; unit tests for all helpers and `forward`. Feature [coder/demo-goal.md](../../../../docs/ft/coder/demo-goal.md). (tddy-demo-runner)
