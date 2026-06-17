#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "$REPO_ROOT"

output_root="${1:-target/termviz-emulator-recordings/fixtures-$(date +%Y%m%d-%H%M%S)}"
mkdir -p "$output_root"

run_fixture() {
  local name="$1"
  shift
  printf 'Recording fixture %s...\n' "$name"
  "$SCRIPT_DIR/record-emulator-demo.sh" "$output_root/$name" -- "$@"
}

run_fixture latency \
  target/debug/termviz examples/latency-demo.csv --x time --y latency --group service
run_fixture throughput \
  target/debug/termviz examples/throughput-demo.csv --x minute --y throughput --group region
run_fixture error-spikes \
  target/debug/termviz examples/error-spikes-demo.csv --x minute --y errors --group service
run_fixture scatter-outliers \
  target/debug/termviz examples/scatter-outliers-demo.csv --x load_ms --y cpu_pct --group node --kind scatter

python3 - "$output_root" <<'PY'
from __future__ import annotations

import json
import sys
from pathlib import Path

root = Path(sys.argv[1])
summary = []
for metrics_path in sorted(root.glob("*/metrics.json")):
    metrics = json.loads(metrics_path.read_text())
    summary.append({
        "fixture": metrics_path.parent.name,
        "frames": metrics["frame_count"],
        "checks": metrics["checks"],
        "latency": metrics["latency"],
        "contact_sheet": metrics["contact_sheet"],
        "video": metrics["video"],
    })

(root / "summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
for item in summary:
    checks = item["checks"]
    failed = [name for name, passed in checks.items() if not passed]
    status = "ok" if not failed else "check"
    print(f"{item['fixture']}: {status} latency={item['latency']['samples_ms']} failed={failed}")
PY

printf 'fixture_recording_dir=%s\n' "$output_root"
printf 'summary=%s/summary.json\n' "$output_root"
