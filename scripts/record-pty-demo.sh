#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "$REPO_ROOT"

if ! command -v tmux >/dev/null 2>&1; then
  echo "recording requires tmux" >&2
  exit 1
fi

output_dir="${1:-}"
if [[ -n "$output_dir" && "$output_dir" != "--" ]]; then
  shift
else
  output_dir="target/termviz-recordings/$(date +%Y%m%d-%H%M%S)"
fi

if [[ "${1:-}" == "--" ]]; then
  shift
  if [[ "$#" -eq 0 ]]; then
    echo "missing command after --" >&2
    exit 1
  fi
  demo_command="$*"
else
  demo_command="target/debug/termviz examples/latency-demo.csv --x time --y latency --group service --protocol blocks"
fi

mkdir -p "$output_dir/frames"
cargo build --quiet

start_epoch="$(date +%s)"
session="termviz-record-$$"
cleanup() {
  tmux kill-session -t "$session" 2>/dev/null || true
}
trap cleanup EXIT

tmux new-session -d -s "$session" -x 120 -y 36 "$demo_command"

for frame in $(seq -w 0 39); do
  tmux capture-pane -t "$session" -p > "$output_dir/frames/frame-${frame}.txt"
  tmux capture-pane -t "$session" -e -p > "$output_dir/frames/frame-${frame}.ansi"
  sleep 0.08
done

cp "$output_dir/frames/frame-39.txt" "$output_dir/final.txt"
cp "$output_dir/frames/frame-39.ansi" "$output_dir/final.ansi"
tmux send-keys -t "$session" q
sleep 0.2

python3 - "$output_dir" "$demo_command" "$start_epoch" <<'PY'
from pathlib import Path
import json
import sys

try:
    from PIL import Image, ImageDraw, ImageFont
except Exception as exc:
    print(f"gif skipped: Pillow unavailable: {exc}", file=sys.stderr)
    raise SystemExit(0)

out = Path(sys.argv[1])
command = sys.argv[2]
started_at = int(sys.argv[3])
frames = sorted((out / "frames").glob("frame-*.txt"))
if not frames:
    raise SystemExit("no captured frames")

font_paths = [
    "/usr/share/fonts/opentype/unifont/unifont.otf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/dejavu/DejaVuSansMono.ttf",
]
font = None
for path in font_paths:
    if Path(path).exists():
        font = ImageFont.truetype(path, 16)
        break
if font is None:
    font = ImageFont.load_default()

cell_w = 9
cell_h = 18
cols = 120
rows = 36
bg = (13, 17, 23)
fg = (203, 213, 225)
images = []
rendered_frames = []

for frame in frames:
    text = frame.read_text(errors="replace").splitlines()
    image = Image.new("RGB", (cols * cell_w, rows * cell_h), bg)
    draw = ImageDraw.Draw(image)
    for y in range(rows):
        line = text[y] if y < len(text) else ""
        if len(line) < cols:
            line = line + " " * (cols - len(line))
        draw.text((0, y * cell_h), line[:cols], font=font, fill=fg)
    images.append(image)
    rendered_frames.append((frame, image))

gif = out / "session.gif"
images[0].save(
    gif,
    save_all=True,
    append_images=images[1:],
    duration=80,
    loop=0,
    optimize=False,
)

keyframe_dir = out / "keyframes"
keyframe_dir.mkdir(exist_ok=True)
keyframe_indexes = sorted({0, len(rendered_frames) // 2, len(rendered_frames) - 1})
keyframes = []
for index in keyframe_indexes:
    source, image = rendered_frames[index]
    path = keyframe_dir / f"frame-{index:02d}.png"
    image.save(path)
    keyframes.append(str(path))

sheet_gap = 16
label_h = 24
sheet_w = len(keyframe_indexes) * images[0].width + (len(keyframe_indexes) - 1) * sheet_gap
sheet_h = images[0].height + label_h
sheet = Image.new("RGB", (sheet_w, sheet_h), bg)
sheet_draw = ImageDraw.Draw(sheet)
for pos, index in enumerate(keyframe_indexes):
    x = pos * (images[0].width + sheet_gap)
    sheet.paste(images[index], (x, label_h))
    sheet_draw.text((x, 2), f"frame {index:02d}", font=font, fill=fg)
contact_sheet = out / "contact-sheet.png"
sheet.save(contact_sheet)

final_text = (out / "final.txt").read_text(errors="replace")
status_protocol_blocks = "protocol: blocks" in final_text
status_protocol_kitty = "protocol: kitty" in final_text
contains_quit_hint = "q quit" in final_text
non_empty_frames = sum(1 for frame, _ in rendered_frames if frame.read_text(errors="replace").strip())
unique_frames = len({frame.read_text(errors="replace") for frame, _ in rendered_frames})

inspection_lines = [
    f"command: {command}",
    f"frames: {len(frames)}",
    f"non_empty_frames: {non_empty_frames}",
    f"unique_text_frames: {unique_frames}",
    f"gif: {gif}",
    f"contact_sheet: {contact_sheet}",
    f"keyframes: {', '.join(keyframes)}",
    f"final_has_q_quit: {str(contains_quit_hint).lower()}",
    f"final_protocol_blocks: {str(status_protocol_blocks).lower()}",
    f"final_protocol_kitty: {str(status_protocol_kitty).lower()}",
]
(out / "inspection.txt").write_text("\n".join(inspection_lines) + "\n")

manifest = {
    "command": command,
    "started_at_epoch": started_at,
    "terminal": {"columns": cols, "rows": rows},
    "frame_count": len(frames),
    "non_empty_frames": non_empty_frames,
    "unique_text_frames": unique_frames,
    "gif": str(gif),
    "contact_sheet": str(contact_sheet),
    "keyframes": keyframes,
    "final_text": str(out / "final.txt"),
    "final_ansi": str(out / "final.ansi"),
    "checks": {
        "final_has_q_quit": contains_quit_hint,
        "final_protocol_blocks": status_protocol_blocks,
        "final_protocol_kitty": status_protocol_kitty,
    },
}
(out / "manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
print(gif)
PY

printf 'recording_dir=%s\n' "$output_dir"
printf 'gif=%s/session.gif\n' "$output_dir"
printf 'contact_sheet=%s/contact-sheet.png\n' "$output_dir"
printf 'inspection=%s/inspection.txt\n' "$output_dir"
printf 'final_text=%s/final.txt\n' "$output_dir"
