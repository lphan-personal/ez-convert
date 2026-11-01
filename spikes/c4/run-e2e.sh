#!/usr/bin/env bash
# Run the C4 end-to-end matrix (`spikes/c4/e2e.rs`).
#
# Thin wrapper over `scripts/c4-tests.sh`: that script is the canonical runner
# (it generates the throwaway cargo package, because `src-tauri` cannot build on a
# host without GTK/WebKit) and it runs BOTH the unit leg and this e2e leg.
#
# Usage:
#   spikes/c4/run-e2e.sh              # e2e only (unit leg skipped)
#   scripts/c4-tests.sh               # unit + e2e (what CI-equivalent runs do)
#
# Environment:
#   EZC_FFMPEG_DIR   directory holding ffmpeg/ffprobe (default: /usr/local/bin)
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
exec "$repo_root/scripts/c4-tests.sh" --e2e-only "$@"
