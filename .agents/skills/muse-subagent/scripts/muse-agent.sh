#!/usr/bin/env bash
# Runs Muse Code headless (`muse exec --json`, one session continued by `--session-id`) on this
# machine and lets a driver follow, judge and resume its turns.
# Usage: muse-agent.sh <command> [args]   (see `muse-agent.sh help`)
#
# The driver brief (references/driver-brief.md) depends on its commands, KEY=VALUE lines and exit
# codes. The host is always `local`.
#
# Output contract for agents: one KEY=VALUE line per fact on stdout, diagnostics on stderr, and a
# distinct exit code for each failure the caller must act on.
set -euo pipefail

die() { local code="$1"; shift; echo "muse-agent: $*" >&2; exit "$code"; }
q() { printf '%q' "$1"; }

EXIT_USAGE=2
EXIT_NOT_LOGGED_IN=10   # the probe turn did not complete — no Meta login or key; the user logs in
EXIT_QUOTA=11           # the probe turn was refused for quota (API 429) — logged in, but nothing runs until it resets
EXIT_BUSY=14            # a turn of that name is still running, or another agent holds the worktree
EXIT_RUN_LOST=16        # RUN_EXIT of a turn killed by a machine stop or reboot — resume its session
EXIT_NO_CLI=19          # `muse` is not on PATH — the user installs Muse Code
EXIT_NO_TOOL=20         # jq (or uuidgen) is missing — the user installs it

RUNS_DIR="$HOME/.muse-runs"   # <name>/stream.jsonl, session-id, prompt-<n>.txt, run-<n>.sh, stderr-<n>.log, exit-<n>

need_local() { [[ "$1" == local ]] || die $EXIT_USAGE "#muse runs on this machine only (host 'local'), not '$1'"; }
valid_name() { [[ "$1" =~ ^[A-Za-z0-9._-]+$ ]] || die $EXIT_USAGE "name must match [A-Za-z0-9._-]+: '$1'"; }

# The final message of a stream's turn: the `run.terminal.*` record's text.
JQ_TERMINAL='select(.payload_type? // "" | startswith("run.terminal.")) | .payload'

# Muse on PATH, jq, and one cheap probe turn — Muse Code has no login-status command, so a turn that
# completes is the proof of a usable login. Prints the default model the probe ran on.
cmd_ensure() {
  need_local "${1:-local}"
  local t missing="" probe out terminal
  for t in jq uuidgen; do command -v $t >/dev/null || missing+="$t "; done
  [[ -z "$missing" ]] || die $EXIT_NO_TOOL "missing on this machine: $missing— the user installs them"
  command -v muse >/dev/null || die $EXIT_NO_CLI "muse is not on PATH — the user installs Muse Code"
  echo "MUSE_VERSION=$(muse --version 2>/dev/null | head -1)"

  probe="$(mktemp -d)"
  out="$(cd "$probe" && muse exec --json --no-session-log --disable-shell --disable-write --disable-web-tools \
    --reasoning-effort minimal "Reply with exactly: OK" 2>"$probe/stderr" </dev/null || true)"
  terminal="$(jq -c "$JQ_TERMINAL" <<<"$out" 2>/dev/null | tail -1)"
  if [[ "$(jq -r '.terminal // ""' <<<"${terminal:-null}")" != completed ]]; then
    tail -5 "$probe/stderr" >&2
    rm -rf "$probe"
    if jq -e '.reason // "" | test("429|rate_limit|quota"; "i")' <<<"${terminal:-null}" >/dev/null 2>&1; then
      echo "MUSE_QUOTA_EXHAUSTED=yes"
      die $EXIT_QUOTA "the probe turn was refused for quota: $(jq -r '.reason' <<<"$terminal")"
    fi
    echo "MUSE_LOGGED_IN=no"
    die $EXIT_NOT_LOGGED_IN "the probe turn did not complete (${terminal:-no terminal record}). The user runs: muse login"
  fi
  echo "MUSE_LOGGED_IN=yes"
  echo "MUSE_MODEL=$(jq -r 'select(.payload_type? == "run.model.configured") | .payload.model_id' <<<"$out" | tail -1)"
  rm -rf "$probe"
  echo "READY=yes"
}

cmd_exec() {
  need_local "${1:?host}"; local dir="${2:?dir}"; shift 2
  [[ $# -gt 0 ]] || die $EXIT_USAGE "exec needs a command"
  bash -c "cd $(q "$dir") && $*"
}

# ── Turns ─────────────────────────────────────────────────────────────────────────────────────────
# A <name> is one Muse session: its UUID is minted on the first turn and stored in
# ~/.muse-runs/<name>/session-id, and every later turn passes it back with --session-id, which
# continues the same conversation. A turn is a detached process in its own process group, so it
# outlives the session that started it and `cancel` takes its children with it.

turns() { ls "$1" 2>/dev/null | grep -c '^prompt-' || true; }

running() {
  local rd="$1" n
  n="$(turns "$rd")"
  [[ $n -gt 0 && ! -f "$rd/exit-$n" && -f "$rd/pid" ]] && kill -0 "$(cat "$rd/pid")" 2>/dev/null
}

# Creates <file> holding <pid> unless a live process already holds it; noclobber makes it atomic.
reserve() {
  local file="$1" pid="$2"
  ( set -C; echo "$pid" >"$file" ) 2>/dev/null && return 0
  kill -0 "$(cat "$file" 2>/dev/null)" 2>/dev/null && return 1
  rm -f "$file"
  ( set -C; echo "$pid" >"$file" ) 2>/dev/null
}

cmd_run() {
  local host="" name="" dir="" prompt_file="" resume="" model="" effort=""
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --host) host="$2"; shift 2 ;;
      --name) name="$2"; shift 2 ;;
      --dir) dir="$2"; shift 2 ;;
      --prompt-file) prompt_file="$2"; shift 2 ;;
      --resume) resume="$2"; shift 2 ;;
      --model) model="$2"; shift 2 ;;
      --effort) effort="$2"; shift 2 ;;
      *) die $EXIT_USAGE "run: unknown argument $1" ;;
    esac
  done
  [[ -n "$host" && -n "$name" && -n "$dir" && -n "$prompt_file" ]] \
    || die $EXIT_USAGE "run needs --host local --name --dir --prompt-file [--model <m>] [--effort <e>] [--resume <session_id>]"
  need_local "$host"; valid_name "$name"
  [[ -f "$prompt_file" ]] || die $EXIT_USAGE "no prompt file $prompt_file"

  local rd="$RUNS_DIR/$name" n lock sid
  mkdir -p "$rd"
  reserve "$rd/launching" $$ || die $EXIT_BUSY "a turn of $name is starting"
  trap "rm -f $(q "$rd/launching")" EXIT
  running "$rd" && die $EXIT_BUSY "a turn of $name is still running"
  if [[ -n "$resume" ]]; then
    [[ ! -f "$rd/session-id" || "$(cat "$rd/session-id")" == "$resume" ]] \
      || die $EXIT_USAGE "$name is session $(cat "$rd/session-id"), not $resume — one name is one session"
    sid="$resume"
  else
    [[ ! -f "$rd/session-id" ]] || die $EXIT_USAGE "$name already has session $(cat "$rd/session-id") — pass --resume, or pick a new name"
    sid="$(uuidgen | tr '[:upper:]' '[:lower:]')"
  fi
  echo "$sid" >"$rd/session-id"
  # One agent per worktree: a lock in the worktree's git dir, held by the turn until it ends.
  lock="$(git -C "$dir" rev-parse --absolute-git-dir)/claude-code-agent.pid"
  reserve "$lock" $$ || die $EXIT_BUSY "another agent holds the worktree $dir (pid $(cat "$lock"))"
  n=$(( $(turns "$rd") + 1 ))
  cp "$prompt_file" "$rd/prompt-$n.txt"
  {
    printf 'MA_DIR=%q MA_LOCK=%q MA_TURN=%q MA_SID=%q MA_MODEL=%q MA_EFFORT=%q MA_PROMPT=%q MA_ERR=%q MA_EXIT=%q\n' \
      "$dir" "$lock" "$n" "$sid" "$model" "$effort" "$rd/prompt-$n.txt" "$rd/stderr-$n.log" "$rd/exit-$n"
    cat <<'RUN'
cd "$MA_DIR" || exit 1
printf '{"type":"muse-agent","event":"turn-start","turn":%s,"at":"%s","session_id":"%s"}\n' "$MA_TURN" "$(date -u +%FT%TZ)" "$MA_SID"
# --yolo: no approval prompts, no sandbox, workspace trusted, no request_user_input — nobody is
# there to answer in a headless turn.
muse exec --json --yolo --workspace "$MA_DIR" --session-id "$MA_SID" \
  ${MA_MODEL:+--model "$MA_MODEL"} ${MA_EFFORT:+--reasoning-effort "$MA_EFFORT"} \
  --prompt-file "$MA_PROMPT" </dev/null 2>"$MA_ERR"
rc=$?
rm -f "$MA_LOCK"
printf '\n{"type":"muse-agent","event":"turn-end","turn":%s,"at":"%s","exit":%d}\n' "$MA_TURN" "$(date -u +%FT%TZ)" $rc
echo $rc >"$MA_EXIT"
RUN
  } >"$rd/run-$n.sh"
  # set -m gives the turn its own process group; nohup lets it outlive the session that started it.
  bash -c 'set -m; nohup bash "$0" >>"$1" 2>&1 </dev/null & echo $! >"$2"; echo $! >"$3"' \
    "$rd/run-$n.sh" "$rd/stream.jsonl" "$rd/pid" "$lock"
  echo "TURN=$n"
  echo "SESSION_ID=$sid"
  echo "NAME=$name"
  echo "WATCH=tail -f $rd/stream.jsonl"
}

# Blocks until the name's current turn has ended, or `secs` pass (a Bash call is capped at 10
# minutes, so a long turn is followed by calling this repeatedly). A turn whose process is gone
# without a turn-end marker was killed by a machine stop: it is marked ended with exit 16.
cmd_wait() {
  need_local "${1:?host}"; local name="${2:?name}" secs="${3:-540}" rd n waited=0
  valid_name "$name"; rd="$RUNS_DIR/$name"
  [[ -d "$rd" ]] || die $EXIT_USAGE "no turns for $name"
  n="$(turns "$rd")"
  while running "$rd"; do
    [[ $waited -ge $secs ]] && { echo WAIT=still-running; echo "TURN=$n"; return 0; }
    sleep 5; waited=$((waited + 5))
  done
  if [[ ! -f "$rd/exit-$n" ]]; then
    printf '\n{"type":"muse-agent","event":"turn-end","turn":%s,"at":"%s","exit":%s}\n' "$n" "$(date -u +%FT%TZ)" $EXIT_RUN_LOST >>"$rd/stream.jsonl"
    echo $EXIT_RUN_LOST >"$rd/exit-$n"
  fi
  echo WAIT=turn-ended; echo "TURN=$n"; echo "RUN_EXIT=$(cat "$rd/exit-$n")"
}

# The stream's records, and those of the last turn only (after its turn-start marker).
JQ_EVENTS='[split("\n")[] | sub("^[^{]*"; "") | (try fromjson catch null) | objects] as $all
  | ($all | map(.type == "muse-agent" and .event == "turn-start") | rindex(true) // -1) as $i
  | ($all[($i + 1):]) as $last'

# The session to resume and how the LAST turn ended.
cmd_summary() {
  need_local "${1:?host}"; local name="${2:?name}" rd n
  valid_name "$name"; rd="$RUNS_DIR/$name"
  [[ -f "$rd/stream.jsonl" ]] || die $EXIT_USAGE "no stream for $name"
  n="$(turns "$rd")"
  jq -rRs --arg sid "$(cat "$rd/session-id" 2>/dev/null)" "$JQ_EVENTS"'
    | ($last | map('"$JQ_TERMINAL"') | last) as $res
    | ($last | map(select(.type == "muse-agent" and .event == "turn-end")) | last) as $fin
    | (($res.text // "") | if test("\\S") then . else ($last | map(select(.payload_type? == "run.output.delta") | .payload.text) | join("")) end) as $shown
    | "SESSION_ID=\($sid)",
      "TURNS=\($all | map(select(.type == "muse-agent" and .event == "turn-start")) | length)",
      "LAST_TURN_STATE=\(if $res then (if $res.terminal == "completed" then "finished" else "error" end)
                         elif $fin then "interrupted" else "running" end)",
      "LAST_TURN_TERMINAL=\($res.terminal // "")",
      "LAST_TURN_EXIT=\($fin.exit // "")",
      "LAST_TURN_TOOL_CALLS=\($last | map(select(.payload_type? == "tool.result")) | length)",
      "RESULT_EMPTY=\(if ($shown | test("\\S")) then "no" else "yes" end)",
      "RESULT<<EOF", ($shown + (if $res.reason then "\n(reason: \($res.reason))" else "" end)), "EOF",
      "LAST_THOUGHT<<EOF", "", "EOF"' "$rd/stream.jsonl"
  # Muse diagnostics land on stderr, not in the stream — show the tail of a turn that did not finish.
  if [[ $n -gt 0 && -s "$rd/stderr-$n.log" ]] && ! grep -q '"run.terminal.completed"' <(tail -c 20000 "$rd/stream.jsonl"); then
    echo "STDERR<<EOF"; tail -5 "$rd/stderr-$n.log"; echo "EOF"
  fi
}

# The last n tool calls and the final message of the current turn — what the agent is doing.
cmd_progress() {
  need_local "${1:?host}"; local name="${2:?name}" n="${3:-15}" rd
  valid_name "$name"; rd="$RUNS_DIR/$name"
  [[ -f "$rd/stream.jsonl" ]] || die $EXIT_USAGE "no stream for $name"
  jq -rRs --argjson n "$n" "$JQ_EVENTS"'
    | $last
    | map(
        if .payload_type? == "tool.result" then
          (.payload.text | try fromjson catch {}) as $r
          | "tool  \($r.command // $r.path // $r.description // "result" | tostring | gsub("\n"; " ") | .[0:160])  exit=\($r.exit_code // "-")"
        elif (.payload_type? // "" | startswith("run.terminal.")) then
          "end   \(.payload.terminal)  \(.payload.text // "" | gsub("\n"; " ") | .[0:160])"
        elif .type == "muse-agent" and .event == "turn-end" then "exit  \(.exit)"
        else empty end)
    | .[-$n:][]' "$rd/stream.jsonl"
}

# Stops the name's current turn: kills its process group, so the agent and its lock go with it.
cmd_cancel() {
  need_local "${1:?host}"; local name="${2:?name}" rd n
  valid_name "$name"; rd="$RUNS_DIR/$name"
  n="$(turns "$rd")"
  if running "$rd"; then
    # muse shuts its tool children down on TERM, which takes a moment; KILL only if it lingers.
    local pgid i
    pgid="$(cat "$rd/pid")"
    kill -TERM -- "-$pgid" 2>/dev/null || true
    for i in 1 2 3 4 5 6 7 8 9 10; do kill -0 -- "-$pgid" 2>/dev/null || break; sleep 1; done
    kill -KILL -- "-$pgid" 2>/dev/null || true
  fi
  if [[ $n -gt 0 && ! -f "$rd/exit-$n" ]]; then
    printf '\n{"type":"muse-agent","event":"turn-end","turn":%s,"exit":130}\n' "$n" >>"$rd/stream.jsonl"
    echo 130 >"$rd/exit-$n"
  fi
  echo "CANCELLED=$name"
}

cmd_help() {
  cat <<'EOF'
usage — <host> is always `local` (Muse Code runs on this machine):
  muse-agent.sh ensure local                        muse and jq on PATH, a probe turn proves the login; prints MUSE_MODEL
  muse-agent.sh exec local <dir> <cmd…>             run a shell command in a dir
  muse-agent.sh run --host local --name <name> --dir <dir> --prompt-file <file> [--model <m>] [--effort <e>] [--resume <session_id>]
                                                    start a turn (`muse exec --json --yolo`, detached) and return;
                                                    the stream is ~/.muse-runs/<name>/stream.jsonl
  muse-agent.sh wait local <name> [secs=540]        block until the turn ends: WAIT=turn-ended (+RUN_EXIT) | still-running
  muse-agent.sh progress local <name> [n=15]        last n tool calls and the final message of the current turn
  muse-agent.sh summary local <name>                SESSION_ID, LAST_TURN_STATE and RESULT of the last turn
  muse-agent.sh cancel local <name>                 kill the current turn
exit codes: 2 usage · 10 not logged in (muse login) · 11 quota exhausted (wait for the reset) · 14 busy · 19 install Muse Code · 20 install jq/uuidgen
RUN_EXIT 16 = the turn was killed by a machine stop or reboot (resume its session)
EOF
}

sub="${1:-help}"; shift || true
case "$sub" in
  ensure) cmd_ensure "$@" ;;
  exec) cmd_exec "$@" ;;
  run) cmd_run "$@" ;;
  wait) cmd_wait "$@" ;;
  progress) cmd_progress "$@" ;;
  summary) cmd_summary "$@" ;;
  cancel) cmd_cancel "$@" ;;
  help|-h|--help) cmd_help ;;
  *) cmd_help >&2; exit $EXIT_USAGE ;;
esac
