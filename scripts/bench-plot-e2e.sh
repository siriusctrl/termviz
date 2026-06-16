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
  scripts/bench-plot-e2e.sh [--quick]

Benchmarks the interactive plot path through a real PTY and Kitty payload stream.
It measures from scripted terminal actions to PTY-observable output, which is
the closest automated proxy for "terminal-visible" latency without external
screen capture hardware or a terminal-specific screenshot API.

Output:
  metric,iteration,duration_ms,payload_delta,payload_bytes_delta,total_bytes_delta,bytes_per_payload

--quick sets iterations to 1.
USAGE
  exit 0
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "plot e2e benchmark skipped: 'python3' is not available." >&2
  exit 0
fi

cargo build --quiet --locked --release
BIN="${REPO_ROOT}/target/release/termviz"
INPUT="${REPO_ROOT}/examples/latency-demo.csv"

python3 - "$BIN" "$INPUT" "$iterations" <<'PY'
import base64
import fcntl
import os
import pty
import re
import select
import signal
import struct
import subprocess
import sys
import termios
import time
from pathlib import Path

BIN = sys.argv[1]
INPUT = sys.argv[2]
ITERATIONS = int(sys.argv[3])
PAYLOAD_RE = re.compile(rb"\x1b_G([^;]*);(.*?)\x1b\\", re.S)


def set_winsize(fd, rows, cols):
    size = struct.pack("HHHH", rows, cols, 0, 0)
    fcntl.ioctl(fd, termios.TIOCSWINSZ, size)


def payload_count(data):
    count = 0
    current = False
    for match in PAYLOAD_RE.finditer(data):
        controls = match.group(1).decode("ascii", "ignore")
        if "a=T" in controls:
            if "t=f" in controls or "m=1" not in controls:
                count += 1
                current = False
            else:
                current = True
        elif current and "m=1" not in controls:
            count += 1
            current = False
    return count


def payload_bytes(data):
    total = 0
    current = []
    for match in PAYLOAD_RE.finditer(data):
        controls = match.group(1).decode("ascii", "ignore")
        chunk = match.group(2).replace(b"\r", b"").replace(b"\n", b"")
        if "a=T" in controls or current:
            if "t=f" in controls:
                path = Path(base64.b64decode(chunk).decode("utf-8", "replace"))
                try:
                    total += path.stat().st_size
                except OSError:
                    pass
                current = []
                continue
            current.append(chunk)
            if "m=1" not in controls:
                try:
                    total += len(base64.b64decode(b"".join(current)))
                finally:
                    current = []
    return total


def read_available(fd, output):
    while True:
        ready, _, _ = select.select([fd], [], [], 0)
        if not ready:
            return
        try:
            chunk = os.read(fd, 65536)
        except BlockingIOError:
            return
        except OSError:
            return
        if not chunk:
            return
        output.extend(chunk)


def wait_for_payload_count(fd, output, target, timeout=8.0):
    deadline = time.perf_counter() + timeout
    while time.perf_counter() < deadline:
        read_available(fd, output)
        current = payload_count(output)
        if current >= target:
            return current, time.perf_counter()
        wait = min(0.001, max(0.0, deadline - time.perf_counter()))
        if wait:
            select.select([fd], [], [], wait)
    read_available(fd, output)
    return payload_count(output), time.perf_counter()


def print_metric(metric, iteration, duration_ms, payload_delta, payload_bytes_delta, total_bytes_delta):
    bytes_per_payload = payload_bytes_delta // payload_delta if payload_delta else 0
    print(
        f"{metric},{iteration},{duration_ms},{payload_delta},"
        f"{payload_bytes_delta},{total_bytes_delta},{bytes_per_payload}",
        flush=True,
    )


class PtySession:
    def __init__(self):
        master, slave = pty.openpty()
        set_winsize(slave, 32, 120)
        flags = fcntl.fcntl(master, fcntl.F_GETFL)
        fcntl.fcntl(master, fcntl.F_SETFL, flags | os.O_NONBLOCK)
        env = os.environ.copy()
        env["KITTY_WINDOW_ID"] = "1"
        env["TERM"] = "xterm-kitty"
        self.master = master
        self.output = bytearray()
        self.proc = subprocess.Popen(
            [
                BIN,
                INPUT,
                "--x",
                "time",
                "--y",
                "latency",
                "--group",
                "service",
                "--protocol",
                "kitty",
            ],
            stdin=slave,
            stdout=slave,
            stderr=slave,
            env=env,
            close_fds=True,
        )
        os.close(slave)

    def send(self, data):
        os.write(self.master, data)

    def resize(self, rows, cols):
        set_winsize(self.master, rows, cols)
        try:
            self.proc.send_signal(signal.SIGWINCH)
        except ProcessLookupError:
            pass

    def close(self):
        try:
            self.send(b"q")
            time.sleep(0.05)
            read_available(self.master, self.output)
        except OSError:
            pass
        if self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=1)
            except subprocess.TimeoutExpired:
                self.proc.kill()
        os.close(self.master)


def run_iteration(iteration):
    session = PtySession()
    try:
        before_payloads = 0
        before_payload_bytes = 0
        before_bytes = 0

        start = time.perf_counter()
        after_payloads, end = wait_for_payload_count(session.master, session.output, 1)
        after_payload_bytes = payload_bytes(session.output)
        after_bytes = len(session.output)
        print_metric(
            "plot_e2e_first_draw",
            iteration,
            int((end - start) * 1000),
            after_payloads - before_payloads,
            after_payload_bytes - before_payload_bytes,
            after_bytes - before_bytes,
        )

        before_payloads = after_payloads
        before_payload_bytes = after_payload_bytes
        before_bytes = after_bytes
        start = time.perf_counter()
        session.send(b"+")
        after_payloads, end = wait_for_payload_count(session.master, session.output, before_payloads + 1)
        after_payload_bytes = payload_bytes(session.output)
        after_bytes = len(session.output)
        print_metric(
            "plot_e2e_key_zoom",
            iteration,
            int((end - start) * 1000),
            after_payloads - before_payloads,
            after_payload_bytes - before_payload_bytes,
            after_bytes - before_bytes,
        )

        before_payloads = after_payloads
        before_payload_bytes = after_payload_bytes
        before_bytes = after_bytes
        start = time.perf_counter()
        session.send(b"\x1b[C")
        after_payloads, end = wait_for_payload_count(session.master, session.output, before_payloads + 1)
        after_payload_bytes = payload_bytes(session.output)
        after_bytes = len(session.output)
        print_metric(
            "plot_e2e_key_pan",
            iteration,
            int((end - start) * 1000),
            after_payloads - before_payloads,
            after_payload_bytes - before_payload_bytes,
            after_bytes - before_bytes,
        )

        before_payloads = after_payloads
        before_payload_bytes = after_payload_bytes
        before_bytes = after_bytes
        start = time.perf_counter()
        session.resize(40, 140)
        after_payloads, end = wait_for_payload_count(session.master, session.output, before_payloads + 1)
        after_payload_bytes = payload_bytes(session.output)
        after_bytes = len(session.output)
        print_metric(
            "plot_e2e_resize",
            iteration,
            int((end - start) * 1000),
            after_payloads - before_payloads,
            after_payload_bytes - before_payload_bytes,
            after_bytes - before_bytes,
        )

        before_payloads = after_payloads
        before_payload_bytes = after_payload_bytes
        before_bytes = after_bytes
        start = time.perf_counter()
        for _ in range(20):
            session.send(b"+")
        time.sleep(0.5)
        read_available(session.master, session.output)
        end = time.perf_counter()
        after_payloads = payload_count(session.output)
        after_payload_bytes = payload_bytes(session.output)
        after_bytes = len(session.output)
        print_metric(
            "plot_e2e_zoom_burst_500ms",
            iteration,
            int((end - start) * 1000),
            after_payloads - before_payloads,
            after_payload_bytes - before_payload_bytes,
            after_bytes - before_bytes,
        )
    finally:
        session.close()


print("metric,iteration,duration_ms,payload_delta,payload_bytes_delta,total_bytes_delta,bytes_per_payload")
for iteration in range(1, ITERATIONS + 1):
    run_iteration(iteration)
PY
