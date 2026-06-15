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
  scripts/bench-metadata-inspect.sh [--quick]

Benchmarks `--inspect` metadata timing for raster, vector, and table inputs.
--quick sets iterations to 1.
USAGE
  exit 0
fi

metric="metadata_inspect"

run_timed() {
  local label="$1"
  shift
  local i=1
  while ((i <= iterations)); do
    local output_file
    output_file="$(mktemp)"

    local start_ns
    local end_ns
    local elapsed_ms
    start_ns="$(date +%s%N)"
    if ! "$@" >"$output_file" 2>&1; then
      echo "Command failed for ${label} (iteration ${i})" >&2
      cat "$output_file" >&2
      rm -f "$output_file"
      exit 1
    fi
    end_ns="$(date +%s%N)"
    elapsed_ms="$(( (end_ns - start_ns) / 1000000 ))"

    printf '%s,%d,%d\n' "$label" "$i" "$elapsed_ms"
    rm -f "$output_file"
    i=$((i + 1))
  done
}

printf 'metric,iteration,duration_ms\n'

cargo build --quiet --locked --release
BIN="${REPO_ROOT}/target/release/termviz"

run_timed "${metric}_png" "$BIN" examples/inspect-square.png --inspect

run_timed "${metric}_svg" "$BIN" examples/inspect.svg --inspect

tmp_csv="$(mktemp)"
cat > "$tmp_csv" <<'EOF'
time,latency
1,20
2,35
3,42
EOF
run_timed "${metric}_csv" "$BIN" "$tmp_csv" --inspect
rm -f "$tmp_csv"
