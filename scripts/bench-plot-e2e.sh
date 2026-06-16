#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "$REPO_ROOT"

iterations=3
if [[ "${1:-}" == "--quick" ]]; then
  iterations=1
elif [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  cat <<'USAGE'
Usage:
  scripts/bench-plot-e2e.sh [--quick]

Benchmarks the interactive plot path through a real PTY and Kitty payload stream.
It measures from scripted terminal actions to PTY-observable output, which is
the closest automated proxy for "terminal-visible" latency without external
screen capture hardware or a terminal-specific screenshot API.

Output:
  metric,iteration,duration_ms,payload_delta,payload_bytes_delta,total_bytes_delta,bytes_per_payload

--quick sets iterations to 1.
USAGE
  exit 0
fi

if ! command -v tmux >/dev/null 2>&1; then
  echo "plot e2e benchmark skipped: 'tmux' is not available." >&2
  exit 0
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "plot e2e benchmark skipped: 'python3' is not available." >&2
  exit 0
fi

cargo build --quiet --locked --release
BIN="${REPO_ROOT}/target/release/termviz"
INPUT="${REPO_ROOT}/examples/latency-demo.csv"

printf 'metric,iteration,duration_ms,payload_delta,payload_bytes_delta,total_bytes_delta,bytes_per_payload\n'

now_ns() {
  date +%s%N
}

file_size() {
  local path="$1"
  if [[ -f "$path" ]]; then
    wc -c < "$path" | tr -d ' '
  else
    printf '0'
  fi
}

payload_count() {
  local path="$1"
  python3 - "$path" <<'PY'
import re
import sys
from pathlib import Path

path = Path(sys.argv[1])
if not path.exists():
    print(0)
    raise SystemExit

data = path.read_bytes()
count = 0
for match in re.finditer(rb'\x1b_G([^;]*);(.*?)\x1b\\', data, re.S):
    controls = match.group(1).decode("ascii", "ignore")
    if "a=T" in controls and "f=100" in controls:
        count += 1
print(count)
PY
}

payload_bytes() {
  local path="$1"
  python3 - "$path" <<'PY'
import base64
import re
import sys
from pathlib import Path

path = Path(sys.argv[1])
if not path.exists():
    print(0)
    raise SystemExit

data = path.read_bytes()
total = 0
current = []
for match in re.finditer(rb'\x1b_G([^;]*);(.*?)\x1b\\', data, re.S):
    controls = match.group(1).decode("ascii", "ignore")
    chunk = match.group(2).replace(b"\r", b"").replace(b"\n", b"")
    if "f=100" in controls or current:
        current.append(chunk)
        if "m=1" not in controls:
            try:
                total += len(base64.b64decode(b"".join(current)))
            finally:
                current = []
print(total)
PY
}

print_metric() {
  local metric="$1"
  local iteration="$2"
  local duration_ms="$3"
  local payload_delta="$4"
  local payload_bytes_delta="$5"
  local total_bytes_delta="$6"
  local bytes_per_payload=0
  if (( payload_delta > 0 )); then
    bytes_per_payload="$(( payload_bytes_delta / payload_delta ))"
  fi
  printf '%s,%d,%d,%d,%d,%d,%d\n' \
    "$metric" "$iteration" "$duration_ms" "$payload_delta" "$payload_bytes_delta" "$total_bytes_delta" "$bytes_per_payload"
}

wait_for_payload_count() {
  local path="$1"
  local target="$2"
  local deadline_ns="$(( $(now_ns) + 8000000000 ))"
  local current=0
  while (( $(now_ns) < deadline_ns )); do
    current="$(payload_count "$path")"
    if (( current >= target )); then
      printf '%s' "$current"
      return 0
    fi
    sleep 0.005
  done
  printf '%s' "$current"
  return 1
}

start_session() {
  local session="$1"
  local raw_log="$2"
  local gate="$3"

  tmux new-session -d -s "$session" -x 120 -y 32 \
    "bash -lc 'while [[ ! -f \"$gate\" ]]; do sleep 0.005; done; exec env KITTY_WINDOW_ID=1 TERM=xterm-kitty \"$BIN\" \"$INPUT\" --x time --y latency --group service --protocol kitty'"
  tmux pipe-pane -t "$session" -o "cat > '$raw_log'"
}

run_iteration() {
  local iteration="$1"
  local tmpdir
  tmpdir="$(mktemp -d)"
  local raw_log="${tmpdir}/raw.log"
  local gate="${tmpdir}/start"
  local session="termviz-e2e-$$-${iteration}"

  cleanup() {
    tmux kill-session -t "$session" 2>/dev/null || true
    rm -rf "$tmpdir"
  }
  trap cleanup RETURN

  start_session "$session" "$raw_log" "$gate"

  local before_payloads=0
  local before_payload_bytes=0
  local before_bytes=0
  local start_ns end_ns after_payloads after_payload_bytes after_bytes duration_ms

  start_ns="$(now_ns)"
  touch "$gate"
  after_payloads="$(wait_for_payload_count "$raw_log" 1)"
  end_ns="$(now_ns)"
  after_payload_bytes="$(payload_bytes "$raw_log")"
  after_bytes="$(file_size "$raw_log")"
  duration_ms="$(( (end_ns - start_ns) / 1000000 ))"
  print_metric plot_e2e_first_draw "$iteration" "$duration_ms" \
    "$(( after_payloads - before_payloads ))" \
    "$(( after_payload_bytes - before_payload_bytes ))" \
    "$(( after_bytes - before_bytes ))"

  before_payloads="$after_payloads"
  before_payload_bytes="$after_payload_bytes"
  before_bytes="$after_bytes"
  local target_payloads="$(( before_payloads + 1 ))"
  start_ns="$(now_ns)"
  tmux send-keys -t "$session" +
  after_payloads="$(wait_for_payload_count "$raw_log" "$target_payloads")"
  end_ns="$(now_ns)"
  after_payload_bytes="$(payload_bytes "$raw_log")"
  after_bytes="$(file_size "$raw_log")"
  duration_ms="$(( (end_ns - start_ns) / 1000000 ))"
  print_metric plot_e2e_key_zoom "$iteration" "$duration_ms" \
    "$(( after_payloads - before_payloads ))" \
    "$(( after_payload_bytes - before_payload_bytes ))" \
    "$(( after_bytes - before_bytes ))"

  before_payloads="$after_payloads"
  before_payload_bytes="$after_payload_bytes"
  before_bytes="$after_bytes"
  target_payloads="$(( before_payloads + 1 ))"
  start_ns="$(now_ns)"
  tmux send-keys -t "$session" Right
  after_payloads="$(wait_for_payload_count "$raw_log" "$target_payloads")"
  end_ns="$(now_ns)"
  after_payload_bytes="$(payload_bytes "$raw_log")"
  after_bytes="$(file_size "$raw_log")"
  duration_ms="$(( (end_ns - start_ns) / 1000000 ))"
  print_metric plot_e2e_key_pan "$iteration" "$duration_ms" \
    "$(( after_payloads - before_payloads ))" \
    "$(( after_payload_bytes - before_payload_bytes ))" \
    "$(( after_bytes - before_bytes ))"

  before_payloads="$after_payloads"
  before_payload_bytes="$after_payload_bytes"
  before_bytes="$after_bytes"
  target_payloads="$(( before_payloads + 1 ))"
  start_ns="$(now_ns)"
  tmux resize-window -t "$session" -x 140 -y 40
  after_payloads="$(wait_for_payload_count "$raw_log" "$target_payloads")"
  end_ns="$(now_ns)"
  after_payload_bytes="$(payload_bytes "$raw_log")"
  after_bytes="$(file_size "$raw_log")"
  duration_ms="$(( (end_ns - start_ns) / 1000000 ))"
  print_metric plot_e2e_resize "$iteration" "$duration_ms" \
    "$(( after_payloads - before_payloads ))" \
    "$(( after_payload_bytes - before_payload_bytes ))" \
    "$(( after_bytes - before_bytes ))"

  before_payloads="$after_payloads"
  before_payload_bytes="$after_payload_bytes"
  before_bytes="$after_bytes"
  start_ns="$(now_ns)"
  for _ in $(seq 1 20); do
    tmux send-keys -t "$session" +
  done
  sleep 0.5
  after_payloads="$(payload_count "$raw_log")"
  end_ns="$(now_ns)"
  after_payload_bytes="$(payload_bytes "$raw_log")"
  after_bytes="$(file_size "$raw_log")"
  duration_ms="$(( (end_ns - start_ns) / 1000000 ))"
  print_metric plot_e2e_zoom_burst_500ms "$iteration" "$duration_ms" \
    "$(( after_payloads - before_payloads ))" \
    "$(( after_payload_bytes - before_payload_bytes ))" \
    "$(( after_bytes - before_bytes ))"

  tmux send-keys -t "$session" q
  sleep 0.1
}

for iteration in $(seq 1 "$iterations"); do
  run_iteration "$iteration"
done
