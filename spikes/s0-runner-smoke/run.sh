#!/usr/bin/env bash
# s0-runner-smoke — reference spike.
#
# Purpose: prove the dispatch -> runner -> artifact path works end to end, and
# serve as the template every real spike (S1/S2/S3) copies. It asserts nothing
# about conversion; it asserts the harness.

set -euo pipefail

SPIKE_ID="${SPIKE_ID:-s0-runner-smoke}"
OUT="spikes/${SPIKE_ID}/out"
mkdir -p "$OUT"

echo "== spike ${SPIKE_ID} =="
echo "SPIKE_ARGS=${SPIKE_ARGS:-<none>}"

uname -a | tee "$OUT/uname.txt"
printf 'runner_os=%s\n' "${RUNNER_OS:-unknown}"

echo "-- rust --"
rustc --version | tee "$OUT/rustc.txt"
cargo --version | tee "$OUT/cargo.txt"

echo "-- ffmpeg --"
ffmpeg -version | head -n 4 | tee "$OUT/ffmpeg-version.txt"
ffprobe -version | head -n 1 | tee "$OUT/ffprobe-version.txt"

echo "-- build the skeleton --"
( cd src-tauri && cargo build --all-targets && cargo test ) | tee "$OUT/cargo-build-test.txt"

echo "-- licences --"
bash scripts/license-assert.sh --scan | tee "$OUT/license-assert.txt"

echo "-- trivial media round-trip (harness sanity, not a conversion claim) --"
ffmpeg -v error -f lavfi -i "testsrc=size=320x240:rate=10:duration=1" \
  -pix_fmt yuv420p "$OUT/probe.mp4"
ffprobe -v error -show_entries stream=codec_name,width,height -of default=noprint_wrappers=1 \
  "$OUT/probe.mp4" | tee "$OUT/probe-streams.txt"

grep -q 'codec_name=h264' "$OUT/probe-streams.txt" \
  || { echo "h264 stream missing from synthetic clip" >&2; exit 1; }

# Machine-readable result for the card to quote.
cat >"$OUT/result.json" <<JSON
{
  "spike_id": "${SPIKE_ID}",
  "runner_os": "${RUNNER_OS:-unknown}",
  "spike_args": "${SPIKE_ARGS:-}",
  "steps": ["uname", "rustc", "ffmpeg", "cargo build --all-targets", "cargo test", "license-assert --scan", "lavfi h264 round-trip"],
  "verdict": "pass"
}
JSON

echo "== spike ${SPIKE_ID}: PASS =="
