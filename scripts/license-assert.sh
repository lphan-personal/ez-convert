#!/usr/bin/env bash
# license-assert.sh — LGPL gate for every ffmpeg binary shipped with EZ Convert.
#
# POLICY
#   EZ Convert distributes LGPL-only ffmpeg builds. A bundled ffmpeg whose
#   `-version` configuration advertises --enable-gpl or --enable-nonfree
#   ("enable-gpl is non-zero") is NOT shippable and fails this gate.
#
# USAGE
#   scripts/license-assert.sh --scan       gate the bundled binaries in this repo (CI)
#   scripts/license-assert.sh --selftest   prove the gate rejects GPL and accepts LGPL (CI)
#
# ENV
#   LICENSE_SCAN_ROOT   scan root override (used by --selftest)
#
# Exit codes: 0 = clean, 1 = a bundled ffmpeg is GPL/nonfree, 2 = bad usage.

set -uo pipefail

REPO_ROOT="${LICENSE_SCAN_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"

# Tauri sidecar / vendored-binary conventions. System ffmpeg is deliberately
# NOT scanned: only what we would ship matters.
SEARCH_DIRS=(vendor/ffmpeg src-tauri/binaries binaries resources/ffmpeg)

log() { printf '%s\n' "$*"; }

# Every candidate ffmpeg/ffprobe binary under the search dirs, deduped, sorted.
candidates() {
  local d
  for d in "${SEARCH_DIRS[@]}"; do
    [ -d "${REPO_ROOT}/${d}" ] || continue
    find "${REPO_ROOT}/${d}" -maxdepth 3 -type f \
      \( -name 'ffmpeg' -o -name 'ffmpeg.exe' -o -name 'ffmpeg-*' \
         -o -name 'ffprobe' -o -name 'ffprobe.exe' -o -name 'ffprobe-*' \) -print 2>/dev/null
  done | sort -u
}

# The build configuration of a binary: run it if we can, otherwise read the
# embedded configure flags straight out of the file (cross-platform binaries).
config_of() {
  local bin="$1"
  if [ -x "$bin" ] && [ "${bin%.exe}" = "$bin" ]; then
    "$bin" -version 2>&1 | head -n 8
  else
    # strings/scanner fallback for foreign-architecture binaries (.exe).
    LC_ALL=C grep -a -o -E -- '--enable-[a-z0-9-]+' "$bin" 2>/dev/null | sort -u
  fi
}

scan() {
  local bin cfg gpl found=0 failed=0

  while IFS= read -r bin; do
    [ -n "$bin" ] || continue
    found=$((found + 1))
    cfg="$(config_of "$bin")"

    gpl=0
    grep -q -- '--enable-gpl' <<<"$cfg" && gpl=1
    grep -q -- '--enable-nonfree' <<<"$cfg" && gpl=1

    if [ "$gpl" -ne 0 ]; then
      log "FAIL ${bin#"${REPO_ROOT}/"}: enable-gpl/nonfree is non-zero — not shippable"
      grep -a -o -E -- '--enable-(gpl|nonfree)' <<<"$cfg" | sort -u | sed 's/^/       /'
      failed=$((failed + 1))
    else
      log "OK   ${bin#"${REPO_ROOT}/"}: LGPL-only configuration"
    fi
  done < <(candidates)

  if [ "$found" -eq 0 ]; then
    log "license-assert: no bundled ffmpeg under [${SEARCH_DIRS[*]}] — nothing to gate yet (skeleton stage)"
    return 0
  fi

  log "license-assert: ${found} bundled ffmpeg checked, ${failed} rejected"
  [ "$failed" -eq 0 ]
}

selftest() {
  local rc status=0
  # Global on purpose: the EXIT trap below runs outside this function's scope.
  selftest_tmp="$(mktemp -d)"
  trap 'rm -rf "${selftest_tmp:-}"' EXIT
  tmp="$selftest_tmp"

  mkdir -p "$tmp/vendor/ffmpeg/gpl" "$tmp/vendor/ffmpeg/lgpl"

  cat >"$tmp/vendor/ffmpeg/gpl/ffmpeg" <<'GPL'
#!/usr/bin/env bash
echo "ffmpeg version 7.1 Copyright (c) 2000-2024 the FFmpeg developers"
echo "built with gcc 13 (Ubuntu 13.2.0)"
echo "configuration: --prefix=/usr --enable-gpl --enable-libx264 --enable-shared"
GPL

  cat >"$tmp/vendor/ffmpeg/lgpl/ffmpeg" <<'LGPL'
#!/usr/bin/env bash
echo "ffmpeg version 7.1 Copyright (c) 2000-2024 the FFmpeg developers"
echo "built with gcc 13 (Ubuntu 13.2.0)"
echo "configuration: --prefix=/usr --disable-gpl --enable-shared"
LGPL

  chmod +x "$tmp/vendor/ffmpeg/gpl/ffmpeg" "$tmp/vendor/ffmpeg/lgpl/ffmpeg"

  log "selftest 1/2: GPL build must be REJECTED"
  LICENSE_SCAN_ROOT="$tmp" bash "${BASH_SOURCE[0]}" --scan
  rc=$?
  if [ "$rc" -eq 1 ]; then
    log "selftest 1/2: PASS (gate exited 1 on a GPL build)"
  else
    log "selftest 1/2: FAIL (expected exit 1, got $rc)"
    status=1
  fi

  rm -rf "$tmp/vendor/ffmpeg/gpl"

  log "selftest 2/2: LGPL build must be ACCEPTED"
  LICENSE_SCAN_ROOT="$tmp" bash "${BASH_SOURCE[0]}" --scan
  rc=$?
  if [ "$rc" -eq 0 ]; then
    log "selftest 2/2: PASS (gate exited 0 on an LGPL build)"
  else
    log "selftest 2/2: FAIL (expected exit 0, got $rc)"
    status=1
  fi

  return "$status"
}

case "${1:-}" in
  --scan) scan ;;
  --selftest) selftest ;;
  *)
    log "usage: $0 --scan | --selftest" >&2
    exit 2
    ;;
esac
