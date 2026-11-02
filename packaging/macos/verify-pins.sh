#!/usr/bin/env bash
# verify-pins.sh — check a downloaded DMG against its published digest pin.
#
# Why this exists (warden finding F10, docs/MACOS-SIGNING.md §11): the DMG's own
# contents are pinned twice — the engine pack carries MANIFEST.sha256 inside, and
# both images publish a sidecar with the *image* digest. `Install Engine Pack.command`
# checks the inner manifest, but nothing checked the image bytes, so the install
# path would mount whatever arrived. This is that check, factored out so it can be
# tested on any host (it uses shasum, not hdiutil).
#
# USAGE: verify-pins.sh <dmg> [sidecar]
#   With no sidecar given, it looks for, in order:
#     <dmg>.sha256
#     <dir-of-dmg>/app-dmg.sha256          (the app job's name for it)
#     <dir-of-dmg>/*.dmg.sha256            (any single match)
#
# The sidecar's first field is the expected digest (`shasum` output order). The
# path in the second field is NOT resolved: the app job writes `dist/…` into it,
# which is not where the artifact lands after download.
#
# EXIT: 0 digest matches, 1 mismatch or no pin found, 2 usage.
set -uo pipefail

dmg="${1:?usage: verify-pins.sh <dmg> [sidecar]}"
side="${2:-}"

[ -f "$dmg" ] || { echo "verify-pins: no such file: $dmg" >&2; exit 2; }

if [ -z "$side" ]; then
  dir="$(cd "$(dirname "$dmg")" && pwd)"
  for cand in "$dmg.sha256" "$dir/app-dmg.sha256"; do
    [ -f "$cand" ] && { side="$cand"; break; }
  done
  if [ -z "$side" ]; then
    # shellcheck disable=SC2012 # a glob over digests, not a listing for humans
    matches="$(ls "$dir"/*.dmg.sha256 2>/dev/null | head -2)"
    case "$matches" in
      "") : ;;
      *$'\n'*) echo "verify-pins: FAIL more than one digest pin next to $(basename "$dmg") — refusing to guess" ; exit 1 ;;
      *) side="$matches" ;;
    esac
  fi
fi

if [ -z "$side" ] || [ ! -f "$side" ]; then
  echo "verify-pins: FAIL no digest sidecar for $(basename "$dmg") — refusing to trust unpinned bytes"
  exit 1
fi

expected="$(awk 'NR==1 {print $1}' "$side")"
actual="$(shasum -a 256 "$dmg" | awk '{print $1}')"

case "$expected" in
  [0-9a-f]*) : ;;
  *) echo "verify-pins: FAIL $(basename "$side") does not start with a sha256 digest"; exit 1 ;;
esac
if [ "${#expected}" -ne 64 ]; then
  echo "verify-pins: FAIL $(basename "$side") digest is ${#expected} chars, not 64"
  exit 1
fi

if [ "$actual" = "$expected" ]; then
  printf 'verify-pins: PASS %s\n  sha256 %s\n  pin    %s\n' \
    "$(basename "$dmg")" "$actual" "$(basename "$side")"
  exit 0
fi
printf 'verify-pins: FAIL %s\n  sha256 %s (computed)\n  pin    %s (%s)\n' \
  "$(basename "$dmg")" "$actual" "$expected" "$(basename "$side")"
exit 1
