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
  scripts/bench-render-pipeline.sh [--quick]

Benchmarks the render pipeline for interactive plots and images without relying
on terminal compositor timing. It emits one CSV schema across Kitty and Blocks:

  kind,metric,iteration_count,total_us,mean_total_us,mean_profile_us,mean_load_us,mean_layout_us,mean_display_list_us,mean_raster_us,mean_compose_us,mean_protocol_us,mean_chrome_us,total_bytes,mean_bytes,mean_chrome_bytes,mean_commands,mean_image_pixels

--quick sets iterations to 3.
USAGE
  exit 0
fi

printf 'kind,metric,iteration_count,total_us,mean_total_us,mean_profile_us,mean_load_us,mean_layout_us,mean_display_list_us,mean_raster_us,mean_compose_us,mean_protocol_us,mean_chrome_us,total_bytes,mean_bytes,mean_chrome_bytes,mean_commands,mean_image_pixels\n'

run_perf_test() {
  local test_name="$1"
  TERMVIZ_RENDER_PIPELINE_ITERS="$iterations" \
  TERMVIZ_PLOT_RECOMPUTE_ITERS="$iterations" \
    cargo test --release --quiet "$test_name" -- --ignored --nocapture \
    | awk -F, 'BEGIN { OFS="," }
        /^plot_pipeline_detail,/ { $1="plot"; print }
        /^image_pipeline_detail,/ { $1="image"; print }'
}

run_perf_test plot_recompute_perf
run_perf_test image_render_pipeline_perf
