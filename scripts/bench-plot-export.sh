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
  scripts/bench-plot-export.sh [--quick]

Benchmarks explicit JSON, SVG, and ANSI export timing for table/CSV plot input.
--quick sets iterations to 1.
USAGE
  exit 0
fi

tmp_csv="$(mktemp)"
cat > "$tmp_csv" <<'EOF'
time,latency
1,20
2,40
3,35
4,55
EOF
trap 'rm -f "$tmp_csv"' EXIT

cargo build --quiet --locked --release
BIN="${REPO_ROOT}/target/release/termviz"

printf 'metric,iteration,duration_ms\n'

run_bench() {
  local metric="$1"
  local format="$2"
  local output_file_prefix="$3"
  local i=1
  while ((i <= iterations)); do
    local output_file
    output_file="$(mktemp "${output_file_prefix}.XXXXXX")"
    local start_ns
    local end_ns
    local elapsed_ms
    start_ns="$(date +%s%N)"
    "$BIN" "$tmp_csv" --x time --y latency --format "$format" --output "$output_file"
    end_ns="$(date +%s%N)"
    elapsed_ms="$(( (end_ns - start_ns) / 1000000 ))"
    printf '%s,%d,%d\n' "$metric" "$i" "$elapsed_ms"
    rm -f "$output_file"
    i=$((i + 1))
  done
}

run_bench plot_json_export json "$tmp_csv.json"
run_bench plot_svg_export svg "$tmp_csv.svg"
run_bench plot_ansi_export ansi "$tmp_csv.ansi"
