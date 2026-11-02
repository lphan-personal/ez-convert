#!/usr/bin/env bash
# overlay-probe.sh — run the overlay acceptance measurement on *this* host.
#
# The app measures itself (`--overlay-probe`, src-tauri/src/shell/probe.rs): it
# summons the wheel through the same code path the hotkey and the tray use, records
# the window, the placement, the click-through mechanism and the summon→paint time as
# JSON, and holds the ring on screen. This script adds the one thing the app cannot do
# itself — a photograph of the screen — and decides the verdict from both.
#
# Usage:
#   BIN=src-tauri/target/release/ez-convert bash scripts/overlay-probe.sh [outdir]
#
# Environment:
#   BIN    the app binary (default: target/release/ez-convert[.exe])
#   HOLD   seconds the ring stays on screen (default 8)
#
# Exit codes: 0 measured and every asserted check passed; 1 a check failed; 2 the
# probe produced nothing (the app did not get far enough to measure anything).
#
# A screenshot that cannot be taken (macOS needs the runner's console session; a
# headless Windows service session has no desktop) is RECORDED and does not change
# the verdict — the JSON assertions are the gate. The verdict line says which case it
# was, so nobody has to guess from a green build.
set -uo pipefail

OUTDIR="${1:-overlay-evidence}"
mkdir -p "$OUTDIR"
OUT="$OUTDIR/overlay-facts.json"
SHOT="$OUTDIR/overlay-screen.png"
VERDICT="$OUTDIR/overlay-verdict.txt"
HOLD="${HOLD:-8}"

here="$(cd "$(dirname "$0")/.." && pwd)"
if [ -z "${BIN:-}" ]; then
  BIN="$here/src-tauri/target/release/ez-convert"
  case "$(uname -s)" in MINGW*|MSYS*|CYGWIN*) BIN="$BIN.exe" ;; esac
fi
if [ ! -x "$BIN" ] && [ ! -f "$BIN" ]; then
  echo "overlay-probe: no binary at $BIN" >&2
  exit 2
fi

rm -f "$OUT" "$OUT.ready" "$SHOT"
echo "overlay-probe: host $(uname -s) $(uname -m), binary $BIN"

"$BIN" --overlay-probe "$OUT" --overlay-hold "$HOLD" >"$OUTDIR/overlay-run.log" 2>&1 &
pid=$!

# Wait for the facts: the probe writes them, then the marker, then holds the ring.
for _ in $(seq 1 600); do
  [ -f "$OUT.ready" ] && break
  if ! kill -0 "$pid" 2>/dev/null && [ ! -f "$OUT.ready" ]; then break; fi
  sleep 0.1
done

shot_state="unavailable"
crop_state="not-checked"
if [ -f "$OUT.ready" ]; then
  # Rectangle to photograph: the placement, in *screenshot* pixels. macOS reports the
  # placement in points and captures in pixels, so the scale factor applies.
  rect="$(python3 - "$OUT" <<'PY' 2>/dev/null || true
import json, sys
facts = json.load(open(sys.argv[1]))
scale = facts.get("window", {}).get("scale_factor") or 1.0
p = facts["placement"]
x = int(round(p["x"] * scale)); y = int(round(p["y"] * scale))
w = int(round(p["width"] * scale)); h = int(round(p["height"] * scale))
print(f"{x},{y},{w},{h}")
PY
)"
  case "$(uname -s)" in
    Darwin)
      if screencapture -x "$SHOT" 2>>"$OUTDIR/overlay-run.log"; then shot_state="captured"; fi
      ;;
    MINGW*|MSYS*|CYGWIN*)
      shot_win="$(cygpath -w "$SHOT" 2>/dev/null || echo "$SHOT")"
      if powershell -NoProfile -Command "Add-Type -AssemblyName System.Windows.Forms; Add-Type -AssemblyName System.Drawing; \$b = [System.Windows.Forms.SystemInformation]::VirtualScreen; \$bmp = New-Object System.Drawing.Bitmap \$b.Width, \$b.Height; \$g = [System.Drawing.Graphics]::FromImage(\$bmp); \$g.CopyFromScreen(\$b.X, \$b.Y, 0, 0, \$bmp.Size); \$bmp.Save('$shot_win', [System.Drawing.Imaging.ImageFormat]::Png); \$g.Dispose(); \$bmp.Dispose();" >>"$OUTDIR/overlay-run.log" 2>&1; then
        shot_state="captured"
      fi
      ;;
  esac
  if [ "$shot_state" = "captured" ] && [ -n "${rect:-}" ]; then
    if python3 "$here/scripts/png-nonblank.py" "$SHOT" --crop "$rect" --json >"$OUTDIR/overlay-shot.json" 2>>"$OUTDIR/overlay-run.log"; then
      crop_state="ring-visible"
    else
      crop_state="ring-not-detected"
    fi
  fi
fi

wait "$pid"
probe_code=$?

{
  echo "OVERLAY_PROBE host=$(uname -s)/$(uname -m)"
  echo "OVERLAY_PROBE facts=$OUT exit=$probe_code"
  echo "OVERLAY_PROBE screenshot=$shot_state crop=$crop_state"
  if [ -f "$OUT" ]; then
    python3 - "$OUT" <<'PY'
import json, sys
facts = json.load(open(sys.argv[1]))
print(f"OVERLAY_PROBE ok={str(facts.get('ok')).lower()} render_ms={facts.get('render_ms')} "
      f"cold_render_ms={facts.get('cold_render_ms')} page_ready={str(facts.get('page_ready')).lower()}")
print(f"OVERLAY_PROBE placement=x{facts['placement']['x']} y{facts['placement']['y']} "
      f"monitor={facts['placement']['monitor_index']} placed={str(facts['placement']['placed']).lower()} "
      f"cursor={facts['placement']['cursor_x']},{facts['placement']['cursor_y']}")
hk = facts.get("hotkey") or {}
print(f"OVERLAY_HOTKEY configured={hk.get('configured')} "
      f"registered={hk.get('registered')} state={hk.get('state')}")
print(f"OVERLAY_HOTKEY matches_configured={str(hk.get('matches_configured')).lower()} "
      f"input_posting_trusted={hk.get('input_posting_trusted')} "
      f"delivered_by={hk.get('delivered_by')} "
      f"summons={hk.get('summons_before')}->{hk.get('summons_after')}")
for press in hk.get("presses") or []:
    print(f"OVERLAY_HOTKEY press={press.get('method')} delivered={str(press.get('delivered')).lower()}")
if hk.get("note"):
    print(f"OVERLAY_HOTKEY note={hk['note']}")
for name in facts.get("asserted", []):
    print(f"OVERLAY_CHECK {name}={'pass' if facts['checks'].get(name) else 'FAIL'}")
for name, value in facts.get("checks", {}).items():
    if name not in facts.get("asserted", []):
        print(f"OVERLAY_NOTE {name}={'pass' if value else 'FAIL'}")
PY
  else
    echo "OVERLAY_PROBE ok=false (no facts written)"
  fi
} | tee "$VERDICT"

if [ ! -f "$OUT" ]; then
  echo "overlay-probe: the probe wrote no facts — see $OUTDIR/overlay-run.log" >&2
  exit 2
fi
exit "$probe_code"
