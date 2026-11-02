#!/usr/bin/env bash
# =============================================================================
# sign-app.sh — sign a .app bundle inside-out.
#
# Order is the whole point (docs/MACOS-PACKAGING.md §4): a signature covers the
# exact bytes of everything nested inside it, so any file modified *after* its
# enclosing object was signed invalidates that seal — "code object is not signed
# at all" at notarization, "the app is damaged" for the user.
#
#   1. loose Mach-O files, deepest path first (dylibs, .so, helpers)
#   2. nested bundles, deepest first (.framework, .app, .xpc, .bundle, .appex)
#   3. the outer .app last
#
# Ad-hoc (`--sign -`) is the v1 identity: no Developer ID yet, so no hardened
# runtime, no timestamp. With a Developer ID, set:
#   EZC_SIGN_IDENTITY="Developer ID Application: NAME (TEAMID)"
#   EZC_SIGN_OPTIONS="--options runtime --timestamp"
# and the same loop produces a notarizable bundle. That is the only difference —
# see the doc for the notarize + staple sequence.
#
# USAGE: packaging/macos/sign-app.sh <path-to-app> [<path-to-app> ...]
# EXIT:  0 all signed and verified, 1 failure, 2 usage.
# =============================================================================
set -euo pipefail

[ $# -ge 1 ] || { echo "usage: $0 <app bundle> [more bundles...]" >&2; exit 2; }

IDENTITY="${EZC_SIGN_IDENTITY:--}"
OPTIONS="${EZC_SIGN_OPTIONS:-}"
if [ "$IDENTITY" = "-" ]; then OPTIONS=""; fi

# A failure here must name the object and quote codesign's own words. The first
# version sent both to /dev/null, so run #3 showed "exit code 1" and nothing
# else — no target, no reason.
trap 'echo "sign-app: FAILED at line $LINENO: $BASH_COMMAND" >&2' ERR

sign_one() {
  local target="$1" out
  # shellcheck disable=SC2086
  if ! out="$(codesign --force --sign "$IDENTITY" $OPTIONS "$target" 2>&1)"; then
    echo "codesign FAILED: $target" >&2
    printf '%s\n' "$out" | sed 's/^/    /' >&2
    return 1
  fi
}

sign_bundle() {
  local app="$1"
  [ -d "$app" ] || { echo "not a bundle: $app" >&2; return 1; }

  local work; work="$(mktemp)"
  # Every Mach-O regular file in the bundle, deepest first. `|| true` because an
  # empty match is grep's exit 1, which pipefail would turn into a silent abort.
  find "$app" -type f -print0 \
    | xargs -0 file 2>/dev/null \
    | grep -E 'Mach-O' \
    | cut -d: -f1 \
    | awk -F/ '{print NF" "$0}' | sort -rn | cut -d' ' -f2- > "$work" || true
  if [ ! -s "$work" ]; then
    echo "no Mach-O files found under $app — nothing to sign, refusing" >&2
    return 1
  fi
  echo "  $(wc -l < "$work" | tr -d ' ') Mach-O files (deepest first)"

  local count=0
  while IFS= read -r file; do
    sign_one "$file"
    count=$((count + 1))
  done < "$work"
  echo "signed $count nested Mach-O files in $(basename "$app")"

  # Nested bundles, deepest first: a bundle's own seal must be in place before
  # its parent is signed.
  find "$app" -mindepth 1 \
    \( -name '*.framework' -o -name '*.app' -o -name '*.appex' -o -name '*.xpc' -o -name '*.bundle' \) \
    -type d -print0 2>/dev/null \
    | awk -F/ '{print NF" "$0}' | sort -rn | cut -d' ' -f2- > "$work.bundles" || true
  while IFS= read -r nested; do
    [ -n "$nested" ] || continue
    sign_one "$nested"
    echo "  signed nested bundle $nested"
  done < "$work.bundles"

  # The outer bundle last, then verify the seal.
  sign_one "$app"
  codesign --verify --deep --strict --verbose=2 "$app" 2>&1 | sed 's/^/  /'
  rm -f "$work" "$work.bundles"
}

for app in "$@"; do
  echo "=== $app (identity: $IDENTITY)"
  sign_bundle "$app"
done
echo "sign-app: OK ($# bundle(s), identity $IDENTITY)"
