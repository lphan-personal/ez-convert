#!/usr/bin/env bash
# =============================================================================
# verify-engine-pack.sh — accept an engine pack the way a user does.
#
# `scripts/mac-engine-pack.sh` proves it can *build* a pack. This proves the
# artifact that came out of it is still whole after leaving the build tree: that
# it downloads, unpacks, keeps its symlinks, still carries a valid (ad-hoc, v1)
# signature on every Mach-O object, matches its version/checksum manifest, and
# lands where the app looks for it.
#
# Every failed check is collected and printed; nothing stops at the first one.
#
# MODES
#   --dmg <file>        mount a published engine-pack DMG and verify its payload
#                       plus the sidecar manifest / .sha256 next to it
#   --pack <dir>        verify an unpacked or installed pack root (the directory
#                       that holds engines/, licenses/, MANIFEST.json)
#   --app <bundle>      additionally assert the Tauri app bundle does NOT carry
#                       the engines (no Contents/Resources/engines, no
#                       LibreOffice.app under Resources) — the whole reason the
#                       pack is external
#
# OPTIONS
#   --expect-arch arm64|x86_64   assert the manifest's arch
#   --skip-exec                  (CI diagnostics only) skip the engine smoke runs
#
# NEEDS macOS: hdiutil, codesign, file(1). Not a Linux script — it fails loudly.
#
# USAGE
#   bash scripts/verify-engine-pack.sh --dmg dist/EZ-Convert-Engine-Pack-arm64.dmg --expect-arch arm64
#   bash scripts/verify-engine-pack.sh --pack "$HOME/Library/Application Support/app.ezconvert.desktop" \
#                                      --app "/Applications/EZ Convert.app"
#
# EXIT: 0 all checks passed, 1 a check failed, 2 usage/environment.
# =============================================================================
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PYTHON="$REPO_ROOT/scripts/pack-manifest.py"

DMG=""
PACK=""
APP=""
EXPECT_ARCH=""
SKIP_EXEC=0

while [ $# -gt 0 ]; do
  case "$1" in
    --dmg) DMG="${2:?--dmg needs a file}"; shift 2 ;;
    --pack) PACK="${2:?--pack needs a directory}"; shift 2 ;;
    --app) APP="${2:?--app needs an .app bundle}"; shift 2 ;;
    --expect-arch) EXPECT_ARCH="${2:?--expect-arch needs arm64 or x86_64}"; shift 2 ;;
    --skip-exec) SKIP_EXEC=1; shift ;;
    -h|--help) sed -n '2,40p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

[ -n "$DMG" ] || [ -n "$PACK" ] || { echo "usage: $0 --dmg <file> | --pack <dir> [--expect-arch A] [--app <bundle>]" >&2; exit 2; }
[ "$(uname -s)" = "Darwin" ] || { echo "this script verifies a macOS artifact (needs hdiutil/codesign)" >&2; exit 2; }
command -v python3 >/dev/null || { echo "python3 is required (pack-manifest.py)" >&2; exit 2; }

FAILS=0
pass() { printf 'PASS  %s\n' "$*"; }
fail() { printf 'FAIL  %s\n' "$*" >&2; FAILS=$((FAILS + 1)); }
note() { printf '      %s\n' "$*"; }

# ------------------------------------------------------------------ manifests
# The pair is checked first: if the manifests do not describe this tree, no
# later check means anything.
check_manifest() { # <pack-root>
  local root="$1"
  local out
  local archargs=()
  if [ -n "$EXPECT_ARCH" ]; then archargs=(--expect-arch "$EXPECT_ARCH"); fi
  if out="$(python3 "$PYTHON" verify --pack "$root" "${archargs[@]}" 2>&1)"; then
    echo "$out" | while IFS= read -r line; do pass "$line"; done
  else
    fail "manifest verification failed for $root"
    printf '%s\n' "$out" | sed 's/^/      /' >&2
  fi
}

# The sidecar is what a *download* can be checked against before anything is
# mounted. A DMG cannot contain its own digest, so this is the only place the
# published bytes are pinned.
check_sidecar() { # <dmg>
  local dmg="$1" side="$1.manifest.json" shafile="$1.sha256"
  [ -f "$side" ] || { fail "no sidecar manifest next to $(basename "$dmg")"; return; }
  [ -f "$shafile" ] || { fail "no .sha256 next to $(basename "$dmg")"; return; }

  local actual recorded_side recorded_file
  actual="$(shasum -a 256 "$dmg" | awk '{print $1}')"
  recorded_file="$(awk '{print $1}' "$shafile")"
  recorded_side="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["artifact"]["sha256"])' "$side")"
  [ "$actual" = "$recorded_file" ] && pass "artifact digest matches $(basename "$shafile")" \
    || fail "artifact digest $actual != $(basename "$shafile") $recorded_file"
  [ "$actual" = "$recorded_side" ] && pass "artifact digest matches the sidecar manifest" \
    || fail "artifact digest $actual != sidecar $recorded_side"

  local version arch
  version="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["pack"]["version"])' "$side")"
  arch="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["pack"]["arch"])' "$side")"
  pass "sidecar version=$version arch=$arch"
  [ -z "$EXPECT_ARCH" ] || [ "$arch" = "$EXPECT_ARCH" ] \
    || fail "sidecar arch $arch != expected $EXPECT_ARCH"
}

# ----------------------------------------------------------------- signatures
# Every Mach-O object in the pack must carry a valid signature. On Apple Silicon
# an unsigned arm64 binary does not execute at all; a *broken* signature is the
# subtler failure, and this is what catches a file touched after signing.
check_signatures() { # <pack-root>
  local root="$1" list offenders=0 count=0
  list="$(mktemp)"
  find "$root" -type f -print0 2>/dev/null \
    | xargs -0 file 2>/dev/null | grep -E 'Mach-O' \
    | sed -E 's/ \(for architecture [^)]*\)//g' | cut -d: -f1 | sort -u > "$list" || true
  while IFS= read -r f; do
    count=$((count + 1))
    if ! codesign --verify "$f" >/dev/null 2>&1; then
      offenders=$((offenders + 1))
      [ "$offenders" -le 20 ] && fail "unsigned or broken signature: ${f#"$root"/}"
    fi
  done < "$list"
  if [ "$count" -eq 0 ]; then
    fail "no Mach-O objects found under $root — wrong path?"
  elif [ "$offenders" -eq 0 ]; then
    pass "$count Mach-O objects verify (ad-hoc v1 signatures)"
  else
    fail "$offenders of $count Mach-O objects failed codesign --verify"
  fi
  rm -f "$list"
}

# --------------------------------------------------------------------- symlinks
# The manifest carries a symlink inventory, and pack-manifest.py compares it with
# the tree. This adds the one thing the inventory cannot show by itself: a spot
# check that the *engine* really works through those links (`program/../Resources`
# and `../Frameworks` are how LibreOffice finds its data).
check_symlinks() { # <pack-root>
  local root="$1" n
  n="$(find "$root" -type l 2>/dev/null | wc -l | tr -d ' ')"
  [ "$n" -gt 0 ] && pass "$n symlinks present after unpack" \
    || fail "no symlinks in the unpacked pack (they were flattened)"
  if [ -L "$root/engines/soffice/Resources" ]; then
    pass "engines/soffice/Resources is a symlink -> $(readlink "$root/engines/soffice/Resources")"
  else
    fail "engines/soffice/Resources is not a symlink (LibreOffice cannot find ../Resources)"
  fi
}

# ------------------------------------------------------------------- smoke runs
check_exec() { # <pack-root>
  local root="$1" out
  [ "$SKIP_EXEC" = "1" ] && { note "engine smoke runs skipped (--skip-exec)"; return; }

  out="$("$root/engines/vips/vips" --version 2>&1)" \
    && pass "pack vips runs: $out" || fail "pack vips does not run: $out"
  out="$("$root/engines/ffmpeg/ffmpeg" -version 2>&1 | head -1)" \
    && pass "pack ffmpeg runs: $out" || fail "pack ffmpeg does not run: $out"
  if "$root/engines/ffmpeg/ffmpeg" -buildconf 2>/dev/null | grep -qE -- '--enable-gpl|--enable-nonfree'; then
    fail "shipped ffmpeg advertises GPL components"
  else
    pass "shipped ffmpeg is not GPL"
  fi
  # soffice through its symlinks is the symlink test with consequences: it loads
  # ../Resources and ../Frameworks or it dies.
  out="$("$root/engines/soffice/program/soffice" --headless --version 2>&1 | head -1)"
  [ -n "$out" ] && pass "pack soffice runs: $out" || fail "pack soffice does not run"
}

# ------------------------------------------------------- the app stays engine-free
check_app() { # <app bundle>
  local app="$1"
  [ -d "$app" ] || { fail "app bundle not found: $app"; return; }
  codesign --verify --strict "$app" >/dev/null 2>&1 \
    && pass "app bundle signature verifies ($(basename "$app"))" \
    || fail "app bundle signature does not verify: $app"
  for forbidden in "Contents/Resources/engines" "Contents/Resources/LibreOffice.app" \
                   "Contents/Resources/soffice" "Contents/Resources/vips" "Contents/Resources/ffmpeg"; do
    if [ -e "$app/$forbidden" ]; then
      fail "engine tree inside the app bundle: $forbidden (the pack must stay external)"
    else
      pass "app bundle carries no $forbidden"
    fi
  done
  if [ -d "$app/Contents/Resources/engines" ]; then
    fail "Contents/Resources/engines exists — engines were bundled despite the pack"
  fi
}

# ------------------------------------------------------- no build-machine paths
# The pack has to run on a Mac with no Homebrew installed: every Mach-O reference
# must be a system library or something inside the pack. This is what stops a
# "works on the build machine" ffmpeg or libvips from shipping.
check_no_build_paths() { # <pack-root>
  local root="$1" list leftovers total
  list="$(mktemp)"; leftovers="$(mktemp)"
  find "$root" -type f -print0 2>/dev/null | xargs -0 file 2>/dev/null | grep -E 'Mach-O' \
    | sed -E 's/ \(for architecture [^)]*\)//g' | cut -d: -f1 | sort -u > "$list"
  while IFS= read -r f; do
    otool -L "$f" 2>/dev/null | tail -n +2 | awk '{print $1}' \
      | grep -E '^/(opt/homebrew|usr/local)/' | sed "s|^|${f#"$root"/}: |" >> "$leftovers"
  done < "$list"
  total="$(wc -l < "$list" | tr -d ' ')"
  if [ -s "$leftovers" ]; then
    fail "$(wc -l < "$leftovers" | tr -d ' ') Mach-O reference(s) still point at the build machine"
    head -20 "$leftovers" | sed 's/^/      /' >&2
  else
    pass "$total Mach-O objects reference only system or in-pack libraries"
  fi
  rm -f "$list" "$leftovers"
}

# ------------------------------------------------------------------------ main
if [ -n "$DMG" ]; then
  [ -f "$DMG" ] || { echo "no such DMG: $DMG" >&2; exit 2; }
  check_sidecar "$DMG"

  MNT="$(mktemp -d)"
  trap 'hdiutil detach "$MNT" >/dev/null 2>&1 || true; rmdir "$MNT" 2>/dev/null || true' EXIT
  hdiutil attach -nobrowse -readonly -mountpoint "$MNT" "$DMG" >/dev/null || { echo "hdiutil attach failed" >&2; exit 1; }

  for rootfile in "Install Engine Pack.command" "README.txt" "payload/MANIFEST.json" "payload/MANIFEST.sha256"; do
    [ -e "$MNT/$rootfile" ] && pass "volume has $rootfile" || fail "volume is missing $rootfile"
  done

  PACK="$MNT/payload"
fi

[ -d "$PACK/engines" ] || { fail "no engines/ under $PACK"; }

check_manifest "$PACK"
check_symlinks "$PACK"
check_signatures "$PACK"
check_no_build_paths "$PACK"
check_exec "$PACK"

if [ -n "$APP" ]; then
  check_app "$APP"
fi

if [ "$FAILS" -ne 0 ]; then
  printf '\nverify-engine-pack: FAIL (%d check(s))\n' "$FAILS" >&2
  exit 1
fi
printf '\nverify-engine-pack: PASS\n'
