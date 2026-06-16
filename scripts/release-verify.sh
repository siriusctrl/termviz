#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "$REPO_ROOT"

printf 'Checking formatting...\n'
cargo fmt --check

printf 'Running tests...\n'
cargo test --locked

printf 'Running clippy...\n'
cargo clippy --all-targets --locked -- -D warnings

printf 'Packaging crate (non-publish safety check)...\n'
cargo package --locked --allow-dirty

if [[ "${TERMVIZ_DRY_RUN_PUBLISH:-0}" == "1" ]]; then
  printf 'Running publish dry-run...\n'
  cargo publish --locked --dry-run
fi

printf 'Building release binary for smoke checks...\n'
cargo build --quiet --locked --release
BIN="${REPO_ROOT}/target/release/termviz"

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

cat >"${tmp_dir}/sample.csv" <<'EOF'
time,latency
1,20
2,40
3,35
4,55
EOF

printf 'Running CLI smoke checks...\n'

"$BIN" examples/inspect-square.png --inspect > "${tmp_dir}/inspect-png.out"
grep -q "content=Png" "${tmp_dir}/inspect-png.out"
grep -q "shape=RasterImage" "${tmp_dir}/inspect-png.out"

"$BIN" examples/inspect.svg --inspect > "${tmp_dir}/inspect-svg.out"
grep -q "content=Svg" "${tmp_dir}/inspect-svg.out"

"$BIN" "${tmp_dir}/sample.csv" --inspect > "${tmp_dir}/inspect-csv.out"
grep -q "content=Csv" "${tmp_dir}/inspect-csv.out"
grep -q "shape=DataTable" "${tmp_dir}/inspect-csv.out"

"$BIN" examples/inspect-square.png --output-format ansi --output "${tmp_dir}/square.ansi"
test -s "${tmp_dir}/square.ansi"

"$BIN" "${tmp_dir}/sample.csv" --output-format json --x time --y latency --output "${tmp_dir}/sample.json"
test -s "${tmp_dir}/sample.json"

"$BIN" "${tmp_dir}/sample.csv" --output-format svg --x time --y latency --output "${tmp_dir}/sample.svg"
test -s "${tmp_dir}/sample.svg"

printf 'Running npm package smoke check...\n'
mkdir -p npm/termviz/vendor
cp "$BIN" npm/termviz/vendor/termviz
cp LICENSE npm/termviz/LICENSE
chmod 755 npm/termviz/vendor/termviz
node npm/termviz/bin/termviz.js --version
npm pack --dry-run ./npm/termviz >/dev/null
npm install --prefix "${tmp_dir}/npm-smoke" ./npm/termviz >/dev/null
"${tmp_dir}/npm-smoke/node_modules/.bin/termviz" --version

printf 'Release verification script completed.\n'
