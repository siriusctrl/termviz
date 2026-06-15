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
  scripts/bench-ansi-export.sh [--quick]

Benchmarks explicit ANSI export timing for raster input.
--quick sets iterations to 1.
USAGE
  exit 0
fi

cargo build --quiet --locked --release
BIN="${REPO_ROOT}/target/release/termviz"

printf 'metric,iteration,duration_ms\n'
i=1
while ((i <= iterations)); do
  output_file="$(mktemp)"
  start_ns="$(date +%s%N)"
  "$BIN" examples/inspect-square.png --format ansi --output "$output_file"
  end_ns="$(date +%s%N)"
  elapsed_ms="$(( (end_ns - start_ns) / 1000000 ))"
  printf 'ansi_export_raster,%d,%d\n' "$i" "$elapsed_ms"
  rm -f "$output_file"
  i=$((i + 1))
done
