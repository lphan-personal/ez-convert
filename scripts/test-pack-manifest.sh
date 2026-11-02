#!/usr/bin/env bash
# =============================================================================
# test-pack-manifest.sh — unit tests for scripts/pack-manifest.py.
#
# Runs anywhere with python3 (no macOS, no DMG): it builds a synthetic pack tree
# with the same shape as the real one — `engines/{vips,ffmpeg,soffice}`, a
# LibreOffice-style `Resources -> .` directory symlink and a relative file
# symlink — seals it, then proves the verifier fails closed on every mutation
# that matters. A manifest that only ever passes is not evidence.
#
# This runs in the macOS pack job *before* the ~20-minute build: a manifest bug
# costs seconds there, not a billed macOS run.
#
# EXIT: 0 every case behaved, 1 a case did not, 2 environment.
# =============================================================================
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PY="$REPO_ROOT/scripts/pack-manifest.py"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/ezc-pack-manifest.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

command -v python3 >/dev/null || { echo "python3 required" >&2; exit 2; }

PASSED=0
FAILED=0
ok()   { PASSED=$((PASSED + 1)); printf 'ok    %s\n' "$*"; }
bad()  { FAILED=$((FAILED + 1)); printf 'FAIL  %s\n' "$*" >&2; }

make_pack() { # <dir>
  local d="$1"
  # The macOS pack keeps LibreOffice as the .app TDF ships; `program` is a
  # compatibility symlink to Contents/MacOS, exactly as mac-engine-pack.sh builds it.
  mkdir -p "$d/engines/soffice/LibreOffice.app/Contents/MacOS" \
           "$d/engines/soffice/LibreOffice.app/Contents/Resources/share" \
           "$d/engines/ffmpeg" "$d/engines/vips/lib" "$d/licenses"
  printf 'launcher\n' > "$d/engines/soffice/LibreOffice.app/Contents/MacOS/soffice"
  chmod +x "$d/engines/soffice/LibreOffice.app/Contents/MacOS/soffice"
  printf 'merged\n'   > "$d/engines/soffice/LibreOffice.app/Contents/MacOS/libmergedlo.dylib"
  printf 'rc\n'       > "$d/engines/soffice/LibreOffice.app/Contents/Resources/fundamentalrc"
  ln -s ../Resources/fundamentalrc "$d/engines/soffice/LibreOffice.app/Contents/MacOS/fundamentalrc"
  ln -s "LibreOffice.app/Contents/MacOS" "$d/engines/soffice/program"
  printf 'ffmpeg\n'  > "$d/engines/ffmpeg/ffmpeg"
  printf 'ffprobe\n' > "$d/engines/ffmpeg/ffprobe"
  printf 'vips\n'    > "$d/engines/vips/vips"
  printf 'lib\n'     > "$d/engines/vips/lib/libvips.dylib"
  printf 'notices\n' > "$d/licenses/THIRD-PARTY-NOTICES.md"
}

seal() { # <dir>
  python3 "$PY" seal --pack "$1" --arch arm64 --pack-version 1.0.0 \
    --engine soffice=26.8.2 --engine ffmpeg=8.1.1 --engine vips=8.18.0 \
    --source "libreoffice-dmg=sha256:$(printf '%064d' 0)" \
    --created-at 2026-11-02T00:00:00Z >/dev/null
}

expect_ok() { # <name> <dir>
  if out="$(python3 "$PY" verify --pack "$2" 2>&1)"; then ok "$1"; else
    bad "$1 — verify failed unexpectedly:"; printf '%s\n' "$out" | sed 's/^/      /' >&2
  fi
}

expect_fail() { # <name> <dir> <substring>
  local out
  if out="$(python3 "$PY" verify --pack "$2" --quiet 2>&1)"; then
    bad "$1 — verify PASSED on a mutated pack"
  elif printf '%s' "$out" | grep -qF -- "$3"; then
    ok "$1"
  else
    bad "$1 — failed, but not for the expected reason ($3):"; printf '%s\n' "$out" | sed 's/^/      /' >&2
  fi
}

# ---------------------------------------------------------------- the good path
make_pack "$WORK/good"
if seal "$WORK/good"; then ok "seal writes both manifests"
else bad "seal failed"; fi
[ -f "$WORK/good/MANIFEST.json" ] && [ -f "$WORK/good/MANIFEST.sha256" ] \
  && ok "MANIFEST.json and MANIFEST.sha256 both exist" || bad "a manifest is missing"
expect_ok "a fresh, sealed pack verifies" "$WORK/good"
if python3 "$PY" verify --pack "$WORK/good" --expect-arch x86_64 >/dev/null 2>&1; then
  bad "verify accepted the wrong --expect-arch"
else ok "verify rejects a mismatched --expect-arch"; fi

# Two engine versions are recorded per engine, and the entry point must exist.
if grep -q '"version": "26.8.2"' "$WORK/good/MANIFEST.json"; then
  ok "engine versions are recorded in MANIFEST.json"
else bad "engine version missing from MANIFEST.json"; fi
if grep -q '"path": "engines/soffice/program",' "$WORK/good/MANIFEST.json"; then
  ok "symlink inventory is recorded"
else bad "symlink inventory missing"; fi

# ------------------------------------------------------- every mutation fails closed
cp -a "$WORK/good" "$WORK/byte-flip"
printf 'X' >> "$WORK/byte-flip/engines/vips/lib/libvips.dylib"
expect_fail "a flipped byte fails" "$WORK/byte-flip" "checksum mismatch"

cp -a "$WORK/good" "$WORK/flattened"
rm -f "$WORK/flattened/engines/soffice/program"; : > "$WORK/flattened/engines/soffice/program"
expect_fail "a symlink replaced by a file fails" "$WORK/flattened" "symlink gone"

cp -a "$WORK/good" "$WORK/retargeted"
ln -sfn /tmp "$WORK/retargeted/engines/soffice/program"
expect_fail "a retargeted symlink fails" "$WORK/retargeted" "symlink retargeted"

cp -a "$WORK/good" "$WORK/extra"
: > "$WORK/extra/engines/ffmpeg/sneaky"
expect_fail "an unlisted file fails" "$WORK/extra" "not in MANIFEST.sha256"

cp -a "$WORK/good" "$WORK/list-edited"
printf ' %064d  ./extra.txt\n' 0 >> "$WORK/list-edited/MANIFEST.sha256"
expect_fail "an edited MANIFEST.sha256 fails" "$WORK/list-edited" "changed since MANIFEST.json was written"

cp -a "$WORK/good" "$WORK/json-edited"
sed -i.bak 's/"version": "1.0.0"/"version": "9.9.9"/' "$WORK/json-edited/MANIFEST.json"
expect_fail "an edited MANIFEST.json fails" "$WORK/json-edited" "does not match its line"

cp -a "$WORK/good" "$WORK/moved"
mv "$WORK/moved/engines/soffice/LibreOffice.app/Contents/MacOS/soffice" \
   "$WORK/moved/engines/soffice/LibreOffice.app/Contents/MacOS/soffice.moved"
expect_fail "a missing engine entry point fails" "$WORK/moved" "listed but missing"

# Seal refuses a pack whose engine has no entry point (fail closed, not "record
# the version and hope").
make_pack "$WORK/no-ffmpeg"
mv "$WORK/no-ffmpeg/engines/ffmpeg/ffmpeg" "$WORK/no-ffmpeg/engines/ffmpeg/ffmpeg.gone"
if python3 "$PY" seal --pack "$WORK/no-ffmpeg" --arch arm64 --engine ffmpeg=8.1.1 >/dev/null 2>&1; then
  bad "seal accepted a pack with no ffmpeg entry point"
else ok "seal fails closed when an engine entry point is absent"; fi

# --------------------------------------------------------------- artifact sidecar
make_pack "$WORK/art"
seal "$WORK/art"
printf 'dmg-bytes\n' > "$WORK/EZ-Convert-Engine-Pack-arm64.dmg"
if python3 "$PY" seal-artifact --dmg "$WORK/EZ-Convert-Engine-Pack-arm64.dmg" \
     --pack-manifest "$WORK/art/MANIFEST.json" >/dev/null; then
  ok "seal-artifact writes the sidecar"
else bad "seal-artifact failed"; fi
if [ -f "$WORK/EZ-Convert-Engine-Pack-arm64.dmg.manifest.json" ] \
   && grep -q '"arch": "arm64"' "$WORK/EZ-Convert-Engine-Pack-arm64.dmg.manifest.json" \
   && grep -q '"sha256": "' "$WORK/EZ-Convert-Engine-Pack-arm64.dmg.manifest.json"; then
  ok "the sidecar carries version, arch and the artifact digest"
else bad "sidecar missing fields"; fi
printf 'changed\n' > "$WORK/EZ-Convert-Engine-Pack-arm64.dmg"
recorded="$(sed -n 's/^ *"sha256": "\([^"]*\)".*/\1/p' "$WORK/EZ-Convert-Engine-Pack-arm64.dmg.manifest.json" | head -1)"
actual="$(shasum -a 256 "$WORK/EZ-Convert-Engine-Pack-arm64.dmg" | awk '{print $1}')"
[ "$recorded" != "$actual" ] && ok "a changed DMG no longer matches its sidecar digest" \
  || bad "the sidecar digest did not change with the artifact"

printf '\n%d passed, %d failed\n' "$PASSED" "$FAILED"
[ "$FAILED" -eq 0 ]
