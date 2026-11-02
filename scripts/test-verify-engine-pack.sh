#!/usr/bin/env bash
# test-verify-engine-pack.sh — the smoke-run checks must FAIL when an engine
# cannot execute, and must not report PASS over an error message.
#
# Background: on a real Intel Mac the shipped pack (arm64-only) produced
#   FAIL  pack vips does not run: ... Bad CPU type in executable
#   FAIL  pack ffmpeg does not run: ... Bad CPU type in executable
#   PASS  pack soffice runs: ... Bad CPU type in executable      <-- false PASS
# The soffice check only looked for non-empty output. An engine that cannot run
# almost always says *something*, so "output" was never the contract — exit
# status is.
#
# `scripts/verify-engine-pack.sh` is Darwin-only by design (hdiutil/codesign),
# so this test extracts the `check_exec` function out of the shipped file by
# name and exercises it against fake engine trees. It therefore always tests
# the code that ships; if the function is renamed, extraction fails loudly.
#
# USAGE: bash scripts/test-verify-engine-pack.sh      (runs on Linux or macOS)
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
shipped="$here/verify-engine-pack.sh"
[ -f "$shipped" ] || { echo "FAIL: $shipped not found" >&2; exit 2; }

fn="$(awk '/^check_exec\(\) \{/{on=1} on{print} on && /^\}$/{exit}' "$shipped")"
case "$fn" in
  check_exec*) : ;;
  *) echo "FAIL: could not extract check_exec from $shipped" >&2; exit 2 ;;
esac

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

failures=0
check() { # <label> <pass-count> <fail-count> <log>
  local label="$1" want_pass="$2" want_fail="$3" log="$4"
  local got_pass got_fail
  got_pass="$(grep -c '^PASS ' "$log" || true)"
  got_fail="$(grep -c '^FAIL ' "$log" || true)"
  if [ "$got_pass" = "$want_pass" ] && [ "$got_fail" = "$want_fail" ]; then
    printf 'ok   %-42s PASS=%s FAIL=%s\n' "$label" "$got_pass" "$got_fail"
  else
    printf 'FAIL %-42s PASS=%s (want %s) FAIL=%s (want %s)\n' \
      "$label" "$got_pass" "$want_pass" "$got_fail" "$want_fail"
    sed 's/^/       /' "$log"
    failures=$((failures + 1))
  fi
}

# A fake pack whose engines either run (case "good") or fail the way an
# arm64 binary fails on an Intel host (case "cpu").
make_pack() { # <dir> <mode>
  local root="$1" mode="$2" e
  for e in vips/vips ffmpeg/ffmpeg soffice/LibreOffice.app/Contents/MacOS/soffice; do
    mkdir -p "$root/engines/$(dirname "$e")"
    case "$mode" in
      good)
        cat >"$root/engines/$e" <<'SH'
#!/bin/sh
case "$1" in
  -version) echo "ffmpeg version 8.1.1 Copyright (c) the FFmpeg developers";;
  -buildconf) echo "configuration: --disable-gpl";;
  --version) echo "vips-8.18.7";;
  *) echo "LibreOffice 26.8.2.2";;
esac
exit 0
SH
        ;;
      cpu)
        cat >"$root/engines/$e" <<'SH'
#!/bin/sh
echo "Bad CPU type in executable" >&2
exit 1
SH
        ;;
      silent)
        cat >"$root/engines/$e" <<'SH'
#!/bin/sh
exit 0
SH
        ;;
    esac
    chmod +x "$root/engines/$e"
  done
}

run_check_exec() { # <pack-root> <log>
  ( # a subshell so the stub reporters and SKIP_EXEC stay local
    SKIP_EXEC=0
    pass() { printf 'PASS %s\n' "$*"; }
    fail() { printf 'FAIL %s\n' "$*"; }
    note() { printf '      %s\n' "$*"; }
    eval "$fn"
    check_exec "$1"
  ) >"$2" 2>&1
}

make_pack "$work/good" good
run_check_exec "$work/good" "$work/good.log"
check "engines that run" 4 0 "$work/good.log"
grep -q '^PASS pack soffice runs: LibreOffice 26\.8' "$work/good.log" \
  || { echo "FAIL: the soffice PASS line lost its version text"; failures=$((failures + 1)); }

make_pack "$work/cpu" cpu
run_check_exec "$work/cpu" "$work/cpu.log"
check "engines that cannot execute (Intel Mac)" 0 3 "$work/cpu.log"
grep -q '^FAIL pack soffice does not run (exit 1): Bad CPU type in executable' "$work/cpu.log" \
  || { echo "FAIL: soffice did not report its exit status and error"; failures=$((failures + 1)); }
grep -q 'licence probe skipped' "$work/cpu.log" \
  || { echo "FAIL: the GPL probe did not admit it could not run"; failures=$((failures + 1)); }

make_pack "$work/silent" silent
run_check_exec "$work/silent" "$work/silent.log"
check "exit 0 with no output" 0 3 "$work/silent.log"

if [ "$failures" -eq 0 ]; then
  echo "test-verify-engine-pack: PASS (3 cases)"
  exit 0
fi
echo "test-verify-engine-pack: FAIL ($failures problem(s))" >&2
exit 1
