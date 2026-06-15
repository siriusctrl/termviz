#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "$REPO_ROOT"

if ! command -v script >/dev/null 2>&1; then
  echo "PTY smoke skipped: 'script' utility is not available."
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
trap 'rm -f "$tmp_csv" "${image_log:-}" "${plot_log:-}"' EXIT

cargo build --quiet --locked --release
BIN="${REPO_ROOT}/target/release/termviz"

image_log="$(mktemp)"
plot_log="$(mktemp)"

printf 'running image pty smoke...\n'
printf 'q\n' | timeout 5s script -q -c "\"$BIN\" examples/inspect-square.png --protocol blocks" "$image_log" >/dev/null

if [[ ! -s "$image_log" ]]; then
  echo "Image PTY smoke produced no session output" >&2
  exit 1
fi

printf 'running plot pty smoke...\n'
printf 'q\n' | timeout 5s script -q -c "\"$BIN\" \"$tmp_csv\" --x time --y latency --protocol blocks" "$plot_log" >/dev/null

if [[ ! -s "$plot_log" ]]; then
  echo "Plot PTY smoke produced no session output" >&2
  exit 1
fi

printf 'pty smoke completed\n'
