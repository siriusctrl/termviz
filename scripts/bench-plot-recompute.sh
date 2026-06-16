#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "$REPO_ROOT"

iterations=12
if [[ "${1:-}" == "--quick" ]]; then
  iterations=3
elif [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  cat <<'USAGE'
Usage:
  scripts/bench-plot-recompute.sh [--quick]

Benchmarks the interactive plot recompute pipeline without starting a terminal.
It runs the ignored plot_recompute_perf test under the release profile and emits:

  metric,iteration_count,total_ms,mean_us,total_bytes,mean_bytes

--quick sets iterations to 3.
USAGE
  exit 0
fi

printf 'metric,iteration_count,total_ms,mean_us,total_bytes,mean_bytes\n'
TERMVIZ_PLOT_RECOMPUTE_ITERS="$iterations" \
  cargo test --release --quiet plot_recompute_perf -- --ignored --nocapture \
  | awk -F, '/^plot_recompute,/ {print $2 "," $3 "," $4 "," $5 "," $6 "," $7}'
