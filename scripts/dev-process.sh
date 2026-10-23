#!/usr/bin/env bash
# Process handling for ./web-dev that works on the bash a developer's shell actually runs — on macOS
# that is /bin/bash 3.2 (#!/usr/bin/env bash finds it before any nix bash). Sourced; it declares
# functions and runs nothing.
#
# Why these exist: ./web-dev used `wait -n` and `fuser -k`. bash 3.2 rejects `wait -n` as an invalid
# option, so the launcher fell straight through to its cleanup the moment Vite started; macOS's
# fuser has no -k, so a daemon left by an earlier run kept its port and answered the readiness check
# in the new one's place — with whatever config it was started with. Both failed silently.
#
# A held port is reported, never killed: what holds it may not be ours. tddy-e2e/tests/
# dev_process_script.rs runs these under the system bash.

# Fail, naming the port, what it is for and — where lsof can tell — the process holding it.
# Probed by connecting (bash's /dev/tcp), so it needs neither lsof nor fuser to decide.
dev_require_free_port() {
  local port="$1" what="$2" holder=""
  if ! (exec 3<>"/dev/tcp/127.0.0.1/${port}") 2>/dev/null; then
    return 0
  fi
  if command -v lsof >/dev/null 2>&1; then
    local pid
    pid=$(lsof -nP -t -iTCP:"$port" -sTCP:LISTEN 2>/dev/null | head -1 || true)
    [ -n "$pid" ] && holder=" by pid ${pid}: $(ps -o command= -p "$pid" 2>/dev/null || true)"
  fi
  echo "Error: port ${port} (${what}) is already in use${holder}" >&2
  echo "  Stop that process (an earlier ./web-dev?) and re-run; ./web-dev does not kill what it did not start." >&2
  return 1
}

# Wait until the first of the given pids exits, and set DEV_EXITED_PID to it — `wait -n` for
# bash 3.2. Polls: there is no portable way to block on "any of these children".
dev_wait_any() {
  DEV_EXITED_PID=""
  while :; do
    local pid
    for pid in "$@"; do
      if ! kill -0 "$pid" 2>/dev/null; then
        DEV_EXITED_PID="$pid"
        return 0
      fi
    done
    sleep 1
  done
}

# A port in the IANA dynamic range that nothing answers on, avoiding any given as arguments. Free
# when probed; the process it is handed to binds it moments later, and a lost race shows as that
# process failing to start, which ./web-dev reports.
dev_free_port() {
  local port taken attempt
  for ((attempt = 0; attempt < 50; attempt++)); do
    port=$((49152 + (RANDOM * 32768 + RANDOM) % 16384))
    for taken in "$@"; do [ "$port" = "$taken" ] && continue 2; done
    if ! (exec 3<>"/dev/tcp/127.0.0.1/${port}") 2>/dev/null; then
      echo "$port"
      return 0
    fi
  done
  echo "Error: no free port found in 49152-65535 after 50 tries." >&2
  return 1
}

# This checkout's own port for <role> (daemon, vite): derived from the checkout's absolute path, so
# every run here gets the same one. The browser keeps its session token and host-key pins per origin,
# and a port that moved on each restart would sign the developer out every time. Used when free and
# not among the remaining arguments; otherwise a random free port, with a notice that this run's
# origin differs. Only the port choice is stable — a held pinned port is still an error elsewhere.
dev_checkout_port() {
  local role="$1" what="$2" crc port taken
  shift 2
  # cksum is POSIX, and the same CRC on macOS and Linux; the path is hashed as bytes.
  crc=$(printf '%s:%s' "$PWD" "$role" | cksum | cut -d ' ' -f 1)
  port=$((49152 + crc % 16384))
  for taken in "$@"; do
    if [ "$port" = "$taken" ]; then
      dev_checkout_port_moved "$port" "$what" "$@"
      return
    fi
  done
  if (exec 3<>"/dev/tcp/127.0.0.1/${port}") 2>/dev/null; then
    dev_checkout_port_moved "$port" "$what" "$@"
    return
  fi
  echo "$port"
}

# dev_checkout_port's fallback: a random free port other than the checkout's own and the avoided.
dev_checkout_port_moved() {
  local preferred="$1" what="$2" port
  shift 2
  port=$(dev_free_port "$preferred" "$@") || return 1
  echo "=== ${what}: this checkout's port ${preferred} is in use; using ${port} for this run ===" >&2
  echo "$port"
}

# The explicit port in a URL (`http://host:5173/x` → 5173), or nothing when it names none.
dev_port_of_url() {
  local authority="${1#*://}"
  authority="${authority%%/*}"
  case "$authority" in
    *:*) local port="${authority##*:}"; [[ "$port" =~ ^[0-9]+$ ]] && echo "$port" ;;
  esac
  return 0
}

# Resolve DAEMON_PORT, VITE_PORT and VITE_URL for a run. A port is this checkout's own ephemeral one
# (dev_checkout_port) unless pinned — DAEMON_PORT, or VITE_PORT / a port in VITE_URL (the OAuth
# redirect base, which Vite must serve exactly) — so runs in different worktrees never collide, and
# one checkout keeps its browser origin across restarts. A pinned port cannot move, so a held one is
# an error (dev_require_free_port).
dev_resolve_ports() {
  if [ -n "${DAEMON_PORT:-}" ]; then
    dev_require_free_port "$DAEMON_PORT" "tddy-daemon (DAEMON_PORT)" || return 1
  else
    DAEMON_PORT=$(dev_checkout_port daemon "tddy-daemon") || return 1
  fi

  local url_port pinned=true
  VITE_URL="${VITE_URL:-}"
  VITE_URL="${VITE_URL%/}"
  url_port=$(dev_port_of_url "$VITE_URL")
  if [ -n "${VITE_PORT:-}" ] && [ -n "$url_port" ] && [ "$VITE_PORT" != "$url_port" ]; then
    echo "Error: VITE_PORT=${VITE_PORT} but VITE_URL=${VITE_URL} names port ${url_port}; Vite can serve only one." >&2
    return 1
  fi
  VITE_PORT="${VITE_PORT:-$url_port}"
  if [ -z "$VITE_PORT" ]; then
    pinned=false
    VITE_PORT=$(dev_checkout_port vite "Vite dev server" "$DAEMON_PORT") || return 1
  fi
  if [ "$pinned" = "true" ] && [ "${HEADLESS:-false}" != "true" ]; then
    dev_require_free_port "$VITE_PORT" "Vite dev server (VITE_PORT / VITE_URL)" || return 1
  fi
  [ -n "$VITE_URL" ] || VITE_URL="http://127.0.0.1:${VITE_PORT}"
  export DAEMON_PORT VITE_PORT VITE_URL
}

# Wait until <url> answers HTTP 200 twice, a second apart (one 200 can come from a server still
# coming up), for at most <timeout_s> seconds. Fails at once when <pid> has exited: a daemon that
# died on its config or lost its port never answers, and running out the timeout only delays the
# error that says why. <what> names the process in the errors.
dev_wait_http_ready() {
  local url="$1" pid="$2" timeout_s="$3" what="$4" deadline code
  deadline=$((SECONDS + timeout_s))
  while [ "$SECONDS" -lt "$deadline" ]; do
    if ! kill -0 "$pid" 2>/dev/null; then
      echo "Error: ${what} exited before answering on ${url}." >&2
      return 1
    fi
    code=$(curl -s -o /dev/null -w '%{http_code}' --connect-timeout 2 --max-time 5 "$url" 2>/dev/null || true)
    if [ "$code" = "200" ]; then
      sleep 1
      code=$(curl -s -o /dev/null -w '%{http_code}' --connect-timeout 2 --max-time 5 "$url" 2>/dev/null || true)
      [ "$code" = "200" ] && return 0
    fi
    sleep 0.5
  done
  echo "Error: ${what} did not respond with HTTP 200 on ${url} within ${timeout_s}s." >&2
  return 1
}

# --- Run record --------------------------------------------------------------------------------
# What a run started, so the next run in this checkout can stop it when the first could not (kill
# -9, a crashed terminal): its own pid, each process group it launched and its LiveKit container.
# Every pid is stored with its start time and acted on only while that still matches — a pid the
# OS has since handed to another process is not ours and is never signalled.
#
#   owner <pid> <start time>     the ./web-dev that wrote the record
#   group <pid> <start time>     a process group it launched (set -m: pid = pgid)
#   container <name>             its LiveKit container

_dev_started_at() {
  ps -o lstart= -p "$1" 2>/dev/null | sed -e 's/^ *//' -e 's/ *$//' || true
}

dev_run_record_init() {
  printf 'owner %s %s\n' "$$" "$(_dev_started_at "$$")" >"$1"
}

dev_run_record_group() {
  printf 'group %s %s\n' "$2" "$(_dev_started_at "$2")" >>"$1"
}

dev_run_record_container() {
  printf 'container %s\n' "$2" >>"$1"
}

# Stop what an earlier run recorded and left behind, then drop its record. A record whose owner is
# still running belongs to a live run, which is an error: two runs in one checkout share target/,
# tmp/ and the record itself.
dev_reap_previous_run() {
  local file="$1" kind id started
  [ -f "$file" ] || return 0
  while read -r kind id started; do
    if [ "$kind" = "owner" ] && [ -n "$started" ] && [ "$(_dev_started_at "$id")" = "$started" ]; then
      echo "Error: ./web-dev is already running in this checkout (pid ${id}, record ${file})." >&2
      echo "  Stop it first; two runs here would share target/, tmp/ and this record." >&2
      return 1
    fi
  done <"$file"

  # Parallel arrays: each stopped pid and the start time it was recorded with.
  local stopped=() stopped_at=()
  while read -r kind id started; do
    case "$kind" in
      group)
        [ -n "$started" ] && [ "$(_dev_started_at "$id")" = "$started" ] || continue
        echo "=== Stopping pid ${id} ($(ps -o command= -p "$id" 2>/dev/null || true)), left by an earlier ./web-dev ===" >&2
        kill -TERM -- "-$id" 2>/dev/null || kill -TERM "$id" 2>/dev/null || true
        stopped+=("$id")
        stopped_at+=("$started")
        ;;
      container)
        case "$id" in
          tddy-web-dev-livekit-*)
            echo "=== Removing LiveKit container ${id}, left by an earlier ./web-dev ===" >&2
            docker rm -f "$id" >/dev/null 2>&1 || true
            ;;
        esac
        ;;
    esac
  done <"$file"

  if [ "${#stopped[@]}" -gt 0 ]; then
    local pid i _
    # Waited on the whole group, not its leader: a leader that dies on TERM can leave members behind.
    for _ in 1 2 3 4 5; do
      local alive=false
      for pid in "${stopped[@]}"; do kill -0 -- "-$pid" 2>/dev/null && alive=true; done
      [ "$alive" = "true" ] || break
      sleep 1
    done
    # Checked again before KILL. A leader still alive must still be the recorded one: an exited
    # leader's pid may already belong to another process. A leader that is gone while its group
    # lives left members behind, and they are still ours — a process-group id cannot be reused
    # while any member of the group is alive.
    for ((i = 0; i < ${#stopped[@]}; i++)); do
      pid="${stopped[$i]}"
      if kill -0 "$pid" 2>/dev/null; then
        [ "$(_dev_started_at "$pid")" = "${stopped_at[$i]}" ] || continue
      fi
      kill -0 -- "-$pid" 2>/dev/null || continue
      kill -KILL -- "-$pid" 2>/dev/null || true
    done
  fi
  rm -f "$file"
}
