#!/usr/bin/env bash
# test-install-engine-pack-arch.sh — the pack installer must refuse the wrong
# architecture, and it must refuse it before it copies anything.
#
# Why this exists: two packs ship (arm64, x86_64) and they are not
# interchangeable. Before this guard, `Install Engine Pack.command` installed the
# arm64 pack on Intel without a word and the app then failed every family with
# `Bad CPU type in executable (os error 86)` — measured on macos-15-intel
# (docs/MACOS-PACKAGING.md §7). The failure the user saw looked like an app bug.
#
# The guard is three lines of `uname -m` vs MANIFEST.json, which is exactly the
# kind of thing that rots silently, so it is tested against a fake pack volume on
# any host (Linux CI included): the real script is copied in unchanged and only
# `ditto` (the copy step) is platform-dependent, so case 1 asserts the guard
# passed, never the install's exit code.
#
# USAGE: bash scripts/test-install-engine-pack-arch.sh      (Linux or macOS)
set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
installer="$here/../packaging/macos/install-engine-pack.command"
[ -f "$installer" ] || { echo "FAIL: $installer not found" >&2; exit 2; }

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

fails=0
ok() { printf 'ok    %s\n' "$*"; }
bad() { printf 'FAIL  %s\n' "$*" >&2; fails=$((fails + 1)); }

# make_volume <arch> -> prints the volume path
make_volume() {
  local arch="$1"
  local vol="$work/vol-$arch"
  mkdir -p "$vol/payload/engines/vips" "$vol/payload/licenses"
  printf 'stub\n' > "$vol/payload/engines/vips/vips"
  printf '{"pack": {"id": "ezconvert-engine-pack-macos-%s", "version": "1.0.0", "arch": "%s"}}\n' \
    "$arch" "$arch" > "$vol/payload/MANIFEST.json"
  ( cd "$vol/payload" && find . -type f ! -name MANIFEST.sha256 | sed 's#^\./##' | sort \
      | while IFS= read -r f; do shasum -a 256 "$f"; done > MANIFEST.sha256 )
  cp "$installer" "$vol/Install Engine Pack.command"
  printf '%s' "$vol"
}

host="$(uname -m)"
case "$host" in
  arm64) other=x86_64 ;;
  x86_64) other=arm64 ;;
  *) echo "FAIL: unexpected host arch '$host'" >&2; exit 2 ;;
esac

# 1. A matching pack passes the guard (it reaches the checksum verification).
vol="$(make_volume "$host")"
out="$(HOME="$work/home-match" bash "$vol/Install Engine Pack.command" 2>&1)"
case "$out" in
  *"architecture: pack $host, this Mac $host — OK"*) ok "matching pack ($host) clears the guard" ;;
  *) bad "matching pack did not clear the guard: $(printf '%s' "$out" | head -2)" ;;
esac
case "$out" in
  *REFUSED*) bad "a matching pack was refused" ;;
esac

# 2. The other architecture is refused, with the message and the remedy.
vol="$(make_volume "$other")"
out="$(HOME="$work/home-mismatch" bash "$vol/Install Engine Pack.command" 2>&1)"; code=$?
case "$out" in
  *"REFUSED: this engine pack is for $other Macs; this Mac is $host."*)
    ok "mismatched pack ($other on $host) is refused" ;;
  *) bad "mismatched pack was not refused: $(printf '%s' "$out" | head -3)" ;;
esac
case "$out" in
  *"Download EZ-Convert-Engine-Pack-$host.dmg"*) ok "the refusal names the right pack to download" ;;
  *) bad "the refusal does not point at EZ-Convert-Engine-Pack-$host.dmg" ;;
esac
[ "$code" -ne 0 ] && ok "refusal exits non-zero ($code)" || bad "refusal exited 0"
case "$out" in
  *"verifying "*) bad "it verified files before refusing (the 9,304-file check must not run first)" ;;
  *) ok "refused before the payload verification" ;;
esac
[ ! -e "$work/home-mismatch/Library/Application Support/app.ezconvert.desktop/engines" ] \
  && ok "nothing was installed" || bad "something was installed despite the refusal"

# 3. No MANIFEST.json: the arch is unknown, which is not a licence to install.
vol="$work/vol-no-manifest"
mkdir -p "$vol/payload/engines/vips"; printf 'stub\n' > "$vol/payload/engines/vips/vips"
cp "$installer" "$vol/Install Engine Pack.command"
out="$(HOME="$work/home-nomanifest" bash "$vol/Install Engine Pack.command" 2>&1)"; code=$?
[ "$code" -ne 0 ] && ok "a pack with no MANIFEST.json is refused" \
  || bad "a pack with no MANIFEST.json was installed"

if [ "$fails" -ne 0 ]; then
  printf '\ntest-install-engine-pack-arch: FAIL (%d)\n' "$fails" >&2
  exit 1
fi
printf '\ntest-install-engine-pack-arch: PASS\n'
