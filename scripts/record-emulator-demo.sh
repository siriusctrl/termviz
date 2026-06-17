#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "$REPO_ROOT"

usage() {
  cat >&2 <<'EOF'
Usage:
  scripts/record-emulator-demo.sh [output-dir] [-- command...]

Runs termviz inside a real Kitty terminal on an Xvfb display, records the
visible terminal window, extracts frames, and writes visual-latency metrics.

The default command is:
  target/debug/termviz examples/latency-demo.csv --x time --y latency --group service
EOF
}

for tool in Xvfb kitty xdotool ffmpeg xwininfo python3; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "recording requires $tool" >&2
    exit 1
  fi
done

output_dir="${1:-}"
if [[ -n "$output_dir" && "$output_dir" != "--" ]]; then
  shift
else
  output_dir="target/termviz-emulator-recordings/$(date +%Y%m%d-%H%M%S)"
fi

if [[ "${1:-}" == "--" ]]; then
  shift
  if [[ "$#" -eq 0 ]]; then
    usage
    exit 1
  fi
  demo_command="$*"
else
  demo_command="target/debug/termviz examples/latency-demo.csv --x time --y latency --group service"
fi

screen_w="${TERMVIZ_EMULATOR_SCREEN_W:-1440}"
screen_h="${TERMVIZ_EMULATOR_SCREEN_H:-960}"
window_w="${TERMVIZ_EMULATOR_WINDOW_W:-1180}"
window_h="${TERMVIZ_EMULATOR_WINDOW_H:-820}"
fps="${TERMVIZ_EMULATOR_FPS:-30}"
class_name="termviz-emulator-$$"
display=""
xvfb_pid=""
kitty_pid=""
ffmpeg_pid=""

mkdir -p "$output_dir/frames" "$output_dir/keyframes"

cleanup() {
  if [[ -n "${ffmpeg_pid:-}" ]]; then
    kill -INT "$ffmpeg_pid" 2>/dev/null || true
    wait "$ffmpeg_pid" 2>/dev/null || true
  fi
  if [[ -n "${kitty_pid:-}" ]]; then
    kill "$kitty_pid" 2>/dev/null || true
    wait "$kitty_pid" 2>/dev/null || true
  fi
  if [[ -n "${xvfb_pid:-}" ]]; then
    kill "$xvfb_pid" 2>/dev/null || true
    wait "$xvfb_pid" 2>/dev/null || true
  fi
}
trap cleanup EXIT

pick_display() {
  for candidate in $(seq 90 119); do
    if [[ ! -e "/tmp/.X11-unix/X${candidate}" ]]; then
      echo ":${candidate}"
      return 0
    fi
  done
  return 1
}

display="$(pick_display)"
if [[ -z "$display" ]]; then
  echo "could not find a free X display" >&2
  exit 1
fi

printf 'Building termviz...\n'
cargo build --quiet

printf 'Starting Xvfb on %s...\n' "$display"
Xvfb "$display" -screen 0 "${screen_w}x${screen_h}x24" +extension GLX +render \
  >"$output_dir/xvfb.log" 2>&1 &
xvfb_pid=$!
sleep 0.5

export DISPLAY="$display"
export LIBGL_ALWAYS_SOFTWARE="${LIBGL_ALWAYS_SOFTWARE:-1}"
export MESA_GL_VERSION_OVERRIDE="${MESA_GL_VERSION_OVERRIDE:-3.3}"
export KITTY_CONFIG_DIRECTORY="$output_dir/kitty-config"
mkdir -p "$KITTY_CONFIG_DIRECTORY"
cat >"$KITTY_CONFIG_DIRECTORY/kitty.conf" <<EOF
font_family DejaVu Sans Mono
font_size 15
remember_window_size no
initial_window_width ${window_w}
initial_window_height ${window_h}
background #0b1117
foreground #cbd5e1
cursor #cbd5e1
enable_audio_bell no
confirm_os_window_close 0
EOF

printf 'Starting Kitty terminal...\n'
kitty --class "$class_name" --title "$class_name" sh -lc "cd '$REPO_ROOT' && exec bash --noprofile --norc" \
  >"$output_dir/kitty.stdout" 2>"$output_dir/kitty.stderr" &
kitty_pid=$!

window_id=""
for _ in $(seq 1 100); do
  window_id="$(xdotool search --onlyvisible --class "$class_name" 2>/dev/null | head -n 1 || true)"
  if [[ -n "$window_id" ]]; then
    break
  fi
  sleep 0.1
done

if [[ -z "$window_id" ]]; then
  echo "could not find Kitty window" >&2
  sed -n '1,160p' "$output_dir/kitty.stderr" >&2 || true
  exit 1
fi

xwininfo -id "$window_id" >"$output_dir/window.txt"
xdotool windowfocus "$window_id" 2>/dev/null || true
xdotool mousemove "$((screen_w - 8))" "$((screen_h - 8))" 2>/dev/null || true
sleep 0.5
xdotool type --clearmodifiers --delay 0 -- "$demo_command"
xdotool key --clearmodifiers Return
sleep 1.2

video="$output_dir/session.mp4"
printf 'Recording %s...\n' "$video"
start_ms="$(date +%s%3N)"
ffmpeg -hide_banner -loglevel warning -y \
  -f x11grab -framerate "$fps" -video_size "${screen_w}x${screen_h}" -i "$display" \
  -c:v libx264 -preset veryfast -crf 18 -pix_fmt yuv420p "$video" \
  >"$output_dir/ffmpeg.stdout" 2>"$output_dir/ffmpeg.stderr" &
ffmpeg_pid=$!

actions_file="$output_dir/actions.jsonl"
: >"$actions_file"
send_key() {
  local name="$1"
  local key="$2"
  local before
  before="$(date +%s%3N)"
  xdotool windowfocus "$window_id" 2>/dev/null || true
  xdotool key --clearmodifiers --window "$window_id" "$key"
  printf '{"name":"%s","key":"%s","sent_ms":%s}\n' "$name" "$key" "$before" >>"$actions_file"
}

sleep 1.2
send_key "zoom_in" "plus"
sleep 0.85
send_key "zoom_in" "plus"
sleep 0.85
send_key "pan_right" "Right"
sleep 0.85
send_key "pan_left" "Left"
sleep 0.85
send_key "zoom_out" "minus"
sleep 0.85
send_key "fit" "0"
sleep 0.8
send_key "quit" "q"
sleep 0.5

kill -INT "$ffmpeg_pid" 2>/dev/null || true
wait "$ffmpeg_pid" 2>/dev/null || true
ffmpeg_pid=""

if [[ ! -s "$video" ]]; then
  echo "recording did not produce a video" >&2
  sed -n '1,160p' "$output_dir/ffmpeg.stderr" >&2 || true
  exit 1
fi

printf 'Extracting frames...\n'
ffmpeg -hide_banner -loglevel error -y -i "$video" "$output_dir/frames/frame-%04d.png"

python3 - "$output_dir" "$demo_command" "$display" "$window_id" "$start_ms" "$fps" \
  "$screen_w" "$screen_h" <<'PY'
from __future__ import annotations

import json
import math
import shutil
import subprocess
import sys
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFont, ImageStat

out = Path(sys.argv[1])
command = sys.argv[2]
display = sys.argv[3]
window_id = sys.argv[4]
start_ms = int(sys.argv[5])
fps = float(sys.argv[6])
screen_w = int(sys.argv[7])
screen_h = int(sys.argv[8])

frames = sorted((out / "frames").glob("frame-*.png"))
if not frames:
    raise SystemExit("no extracted frames")

actions = []
for line in (out / "actions.jsonl").read_text().splitlines():
    if line.strip():
        actions.append(json.loads(line))

def frame_time_ms(index: int) -> float:
    return start_ms + (index * 1000.0 / fps)

def load_rgb(path: Path) -> Image.Image:
    return Image.open(path).convert("RGB")

def luma_stats(image: Image.Image) -> tuple[float, float]:
    gray = image.convert("L")
    stat = ImageStat.Stat(gray)
    mean = float(stat.mean[0])
    dark_pixels = 0
    hist = gray.histogram()
    dark_pixels = sum(hist[:10])
    return mean, dark_pixels / (image.width * image.height)

def diff_ratio(left: Image.Image, right: Image.Image) -> float:
    diff = ImageChops.difference(left, right).convert("L")
    hist = diff.histogram()
    changed = sum(count for value, count in enumerate(hist) if value >= 12)
    return changed / (diff.width * diff.height)

sampled = [load_rgb(path) for path in frames]
luma = [luma_stats(image) for image in sampled]
blank_flags = [mean < 2.0 or dark > 0.995 for mean, dark in luma]
diffs = [0.0]
for prev, current in zip(sampled, sampled[1:]):
    diffs.append(diff_ratio(prev, current))

first_nonblank = next((idx for idx, blank in enumerate(blank_flags) if not blank), None)
plot_command = " --x " in f" {command} " and " --y " in f" {command} "

def saturated_plot_pixel_count(image: Image.Image) -> int:
    count = 0
    data = image.tobytes()
    for offset in range(0, len(data), 3):
        red, green, blue = data[offset], data[offset + 1], data[offset + 2]
        if max(red, green, blue) >= 70 and max(red, green, blue) - min(red, green, blue) >= 35:
            count += 1
    return count

first_nonblank_series_pixels = (
    0 if first_nonblank is None else saturated_plot_pixel_count(sampled[first_nonblank])
)
established_start = first_nonblank if first_nonblank is not None else 0
quit_sent = next((int(action["sent_ms"]) for action in actions if action["name"] == "quit"), None)
quit_frame = (
    None
    if quit_sent is None
    else max(0, min(len(frames) - 1, math.floor((quit_sent - start_ms) * fps / 1000.0)))
)
analysis_end = quit_frame if quit_frame is not None else len(frames)
post_established_blank = [
    idx for idx, blank in enumerate(blank_flags)
    if idx >= established_start + max(1, int(fps * 0.5)) and idx < analysis_end and blank
]
large_delta_frames = [
    idx for idx, ratio in enumerate(diffs)
    if idx >= established_start and idx < analysis_end and ratio >= 0.35
]

action_results = []
visible_threshold = 0.0025
action_frame_indexes = [
    max(0, min(len(frames) - 1, math.floor((int(action["sent_ms"]) - start_ms) * fps / 1000.0)))
    for action in actions
]
for pos, action in enumerate(actions):
    sent = int(action["sent_ms"])
    baseline_index = action_frame_indexes[pos]
    baseline = sampled[baseline_index]
    next_sent = int(actions[pos + 1]["sent_ms"]) if pos + 1 < len(actions) else sent + 1000
    next_frame = (
        action_frame_indexes[pos + 1]
        if pos + 1 < len(action_frame_indexes)
        else math.floor((next_sent - start_ms) * fps / 1000.0)
    )
    search_start = baseline_index + 1
    search_end_margin = 2 if pos + 1 < len(action_frame_indexes) else 1
    search_end = min(
        len(frames) - 1,
        max(search_start, next_frame - search_end_margin),
    )
    visible_index = None
    visible_ratio = 0.0
    if baseline_index > 0:
        baseline_delta = diff_ratio(sampled[baseline_index - 1], baseline)
        if not blank_flags[baseline_index] and baseline_delta >= visible_threshold:
            visible_index = baseline_index
            visible_ratio = baseline_delta
    for idx in range(search_start, search_end + 1):
        if visible_index is not None:
            break
        ratio = diff_ratio(baseline, sampled[idx])
        if not blank_flags[idx] and ratio >= visible_threshold:
            visible_index = idx
            visible_ratio = ratio
            break
    result = dict(action)
    result["baseline_frame"] = baseline_index
    result["first_visible_frame"] = visible_index
    result["visible_latency_ms"] = (
        None if visible_index is None else max(0.0, round(frame_time_ms(visible_index) - sent, 1))
    )
    result["visible_changed_ratio"] = round(visible_ratio, 6)
    action_results.append(result)

latencies = [
    item["visible_latency_ms"]
    for item in action_results
    if item["name"] != "quit" and item["visible_latency_ms"] is not None
]
invisible_non_quit_actions = [
    item["name"]
    for item in action_results
    if item["name"] != "quit" and item["first_visible_frame"] is None
]

keyframe_indexes = {0, max(0, len(frames) // 2), len(frames) - 1}
for item in action_results:
    for key in ("baseline_frame", "first_visible_frame"):
        idx = item.get(key)
        if isinstance(idx, int):
            keyframe_indexes.add(max(0, min(len(frames) - 1, idx)))
keyframe_indexes = sorted(keyframe_indexes)

keyframe_dir = out / "keyframes"
keyframe_paths = []
for idx in keyframe_indexes:
    source = frames[idx]
    dest = keyframe_dir / f"frame-{idx:04d}.png"
    shutil.copyfile(source, dest)
    keyframe_paths.append(str(dest))

font_paths = [
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/dejavu/DejaVuSansMono.ttf",
]
font = ImageFont.load_default()
for path in font_paths:
    if Path(path).exists():
        font = ImageFont.truetype(path, 16)
        break

thumb_w = 360
thumb_h = max(1, round(screen_h * thumb_w / screen_w))
label_h = 28
gap = 12
columns = min(4, max(1, len(keyframe_indexes)))
rows = math.ceil(len(keyframe_indexes) / columns)
sheet = Image.new(
    "RGB",
    (columns * thumb_w + (columns - 1) * gap, rows * (thumb_h + label_h) + (rows - 1) * gap),
    (11, 17, 23),
)
draw = ImageDraw.Draw(sheet)
for pos, idx in enumerate(keyframe_indexes):
    row = pos // columns
    col = pos % columns
    x = col * (thumb_w + gap)
    y = row * (thumb_h + label_h + gap)
    image = sampled[idx].resize((thumb_w, thumb_h), Image.Resampling.LANCZOS)
    sheet.paste(image, (x, y + label_h))
    draw.text((x, y + 5), f"frame {idx:04d}  +{frame_time_ms(idx) - start_ms:.0f}ms", fill=(203, 213, 225), font=font)
contact_sheet = out / "contact-sheet.png"
sheet.save(contact_sheet)

checks = {
    "has_frames": len(frames) > 0,
    "has_nonblank_frame": first_nonblank is not None,
    "plot_first_frame_has_series_pixels": (
        True if not plot_command else first_nonblank_series_pixels >= 1000
    ),
    "no_post_start_blank_frames": len(post_established_blank) == 0,
    "has_visible_non_quit_action_sample": bool(latencies),
    "median_visible_latency_ms_under_150": (
        False if not latencies else sorted(latencies)[len(latencies) // 2] <= 150
    ),
    "max_visible_latency_ms_under_300": (
        False if not latencies else max(latencies) <= 300
    ),
}

metrics = {
    "command": command,
    "display": display,
    "window_id": window_id,
    "screen": {"width": screen_w, "height": screen_h},
    "fps": fps,
    "frame_count": len(frames),
    "video": str(out / "session.mp4"),
    "contact_sheet": str(contact_sheet),
    "keyframes": keyframe_paths,
    "first_nonblank_frame": first_nonblank,
    "first_nonblank_series_pixels": first_nonblank_series_pixels,
    "quit_frame": quit_frame,
    "analysis_frame_end": analysis_end,
    "blank_frames": [idx for idx, blank in enumerate(blank_flags) if blank],
    "post_established_blank_frames": post_established_blank,
    "large_delta_frames": large_delta_frames,
    "max_frame_delta_ratio": round(max(diffs), 6),
    "mean_frame_delta_ratio": round(sum(diffs) / len(diffs), 6),
    "actions": action_results,
    "invisible_non_quit_actions": invisible_non_quit_actions,
    "latency": {
        "samples_ms": latencies,
        "median_ms": None if not latencies else sorted(latencies)[len(latencies) // 2],
        "max_ms": None if not latencies else max(latencies),
    },
    "checks": checks,
}

(out / "metrics.json").write_text(json.dumps(metrics, indent=2, sort_keys=True) + "\n")

inspection = [
    f"command: {command}",
    f"display: {display}",
    f"window_id: {window_id}",
    f"video: {out / 'session.mp4'}",
    f"frames: {len(frames)}",
    f"contact_sheet: {contact_sheet}",
    f"first_nonblank_frame: {first_nonblank}",
    f"first_nonblank_series_pixels: {first_nonblank_series_pixels}",
    f"invisible_non_quit_actions: {invisible_non_quit_actions}",
    f"post_established_blank_frames: {len(post_established_blank)}",
    f"large_delta_frames: {len(large_delta_frames)}",
    f"latency_samples_ms: {latencies}",
    f"latency_median_ms: {metrics['latency']['median_ms']}",
    f"latency_max_ms: {metrics['latency']['max_ms']}",
]
for name, passed in checks.items():
    inspection.append(f"check_{name}: {str(passed).lower()}")
(out / "inspection.txt").write_text("\n".join(inspection) + "\n")

print(out / "session.mp4")
PY

printf 'recording_dir=%s\n' "$output_dir"
printf 'video=%s/session.mp4\n' "$output_dir"
printf 'frames=%s/frames\n' "$output_dir"
printf 'contact_sheet=%s/contact-sheet.png\n' "$output_dir"
printf 'metrics=%s/metrics.json\n' "$output_dir"
printf 'inspection=%s/inspection.txt\n' "$output_dir"
