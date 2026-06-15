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
  scripts/bench-interactive-pty.sh [--quick]

Benchmarks interactive PTY startup and key-path redrawing with scripted controls.
This script validates that the PTY session starts, receives input, and exits with q.
--quick sets iterations to 1.
USAGE
  exit 0
fi

if ! command -v script >/dev/null 2>&1; then
  echo "Interactive PTY benchmark skipped: 'script' utility is not available."
  exit 0
fi

INPUT_PNG="${REPO_ROOT}/examples/inspect-square.png"
if [[ ! -f "$INPUT_PNG" ]]; then
  echo "Interactive PTY benchmark skipped: missing example raster ${INPUT_PNG}" >&2
  exit 0
fi

cargo build --quiet --locked --release
BIN="${REPO_ROOT}/target/release/termviz"

run_bench() {
  local metric="$1"
  local keys="$2"

  local i=1
  while ((i <= iterations)); do
    local session_log
    session_log="$(mktemp)"

    local start_ns
    local end_ns
    local elapsed_ms

    start_ns="$(date +%s%N)"
    if ! printf '%b' "$keys" | timeout 12s script -q "$session_log" -c "\"$BIN\" \"$INPUT_PNG\" --protocol blocks" >/dev/null; then
      echo "Interactive PTY benchmark failed for ${metric} (iteration ${i})" >&2
      cat "$session_log" >&2 || true
      rm -f "$session_log"
      exit 1
    fi
    end_ns="$(date +%s%N)"
    elapsed_ms="$(( (end_ns - start_ns) / 1000000 ))"

    if ! grep -a -Fq "$(printf '\x1b[?1049h')" "$session_log"; then
      echo "Interactive PTY benchmark did not start an alternate-screen session for ${metric} (iteration ${i})" >&2
      cat "$session_log" >&2
      rm -f "$session_log"
      exit 1
    fi
    if ! test -s "$session_log"; then
      echo "Interactive PTY benchmark produced no session output for ${metric} (iteration ${i})" >&2
      rm -f "$session_log"
      exit 1
    fi

    printf '%s,%d,%d\n' "$metric" "$i" "$elapsed_ms"

    rm -f "$session_log"
    i=$((i + 1))
  done
}

printf 'metric,iteration,duration_ms\n'

run_bench "interactive_first_draw" $'q'
run_bench "interactive_pan_zoom" $'+\e[D\e[A\e[C\e[B-0q'
