#!/usr/bin/env bash
# install-from-dmgs.sh — install both shipped DMGs the way a user does.
#
# Used by `.github/workflows/macos-validate.yml` on both architectures (native
# arm64 and Intel x86_64) so the two runs execute the *identical* install path.
# A difference between those runs is then an architecture fact, not a
# difference in how the installer was driven.
#
# USAGE: install-from-dmgs.sh <dir holding the downloaded artifacts> [log dir]
#
# It writes install.log (app) and install-pack.log (engine pack) into the log
# directory (default: the current directory) and exits non-zero on any failure —
# including the classic "installed but the payload is not there" case.
set -euo pipefail

in_dir="${1:?usage: install-from-dmgs.sh <artifact dir> [log dir]}"
log_dir="${2:-.}"
mkdir -p "$log_dir"
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
app_log="$log_dir/install.log"
pack_log="$log_dir/install-pack.log"
: >"$app_log"
: >"$pack_log"

say() { local f="$1"; shift; printf '%s\n' "$*" | tee -a "$f"; }  # say <logfile> <line...>

# The image bytes are pinned by a sidecar digest; check it before mounting, so a
# truncated or swapped download cannot reach /Applications (warden F10,
# docs/MACOS-SIGNING.md §11). The pack's *inner* MANIFEST.sha256 is checked later
# by its own installer; this is only the image-level pin.
verify_pin() { # <dmg> <logfile>
  local dmg="$1" log="$2" out
  if ! out="$(bash "$script_dir/verify-pins.sh" "$dmg" 2>&1)"; then
    printf '%s\n' "$out" | tee -a "$log"
    echo "::error::$(basename "$dmg") does not match its published digest" | tee -a "$log"
    exit 1
  fi
  printf '%s\n' "$out" | tee -a "$log"
}

# ---------------------------------------------------------------- environment
{
  echo "host       $(hostname)"
  echo "uname -m   $(uname -m)"
  echo "sw_vers    $(sw_vers -productVersion) ($(sw_vers -buildVersion))"
  echo "rosetta    $(sysctl -n sysctl.proc_translated 2>/dev/null || echo 'not translated / no sysctl')"
} | tee -a "$app_log"

# ------------------------------------------------------------------- the app
appdmg="$(find "$in_dir" -name 'EZ-Convert-*-macos-universal.dmg' | head -1)"
[ -n "$appdmg" ] || { echo "::error::no app DMG artifact under $in_dir"; exit 1; }
verify_pin "$appdmg" "$app_log"
app_mnt="${RUNNER_TEMP:-/tmp}/app-mnt"
rm -rf "$app_mnt"; mkdir -p "$app_mnt"
hdiutil attach -nobrowse -readonly -mountpoint "$app_mnt" "$appdmg" >/dev/null
# The volume's own contents are part of the install proof: the Applications
# symlink is what "drag to Applications" depends on.
say "$app_log" "volume $(basename "$appdmg"):"
ls -la "$app_mnt" | tee -a "$app_log"

sudo rm -rf "/Applications/EZ Convert.app"
sudo ditto "$app_mnt/EZ Convert.app" "/Applications/EZ Convert.app"
hdiutil detach "$app_mnt" >/dev/null

say "$app_log" "installed app bundle:"
ls -la "/Applications/EZ Convert.app/Contents" | tee -a "$app_log"
say "$app_log" "lipo: $(lipo -archs "/Applications/EZ Convert.app/Contents/MacOS/ez-convert")"
codesign --verify --strict "/Applications/EZ Convert.app" \
  && say "$app_log" "codesign --verify --strict: OK"
say "$app_log" "installed size: $(du -sh "/Applications/EZ Convert.app" | cut -f1)"

# The app must not carry engines (docs/MACOS-PACKAGING.md §1). Assert it here so
# an install-path regression cannot hide behind a pack that happens to be there.
for marker in engines LibreOffice.app soffice vips ffmpeg; do
  if [ -e "/Applications/EZ Convert.app/Contents/Resources/$marker" ]; then
    echo "::error::the installed app carries Contents/Resources/$marker" | tee -a "$app_log"
    exit 1
  fi
done
say "$app_log" "Contents/Resources carries no engine payload: OK"

# ------------------------------------------------------------- the engine pack
# Two packs ship (arm64 and x86_64). Pick by the host architecture: `head -1` over
# a glob is filesystem order, and with both packs in one artifact set that would
# pick at random. The installer repeats the check inside the image, so a lone
# wrong-architecture pack is refused with a message instead of installed.
host_arch="$(uname -m)"
packdmg="$(find "$in_dir" -name "EZ-Convert-Engine-Pack-${host_arch}.dmg" | head -1)"
if [ -z "$packdmg" ]; then
  packs="$(find "$in_dir" -name 'EZ-Convert-Engine-Pack-*.dmg' | sort)"
  n="$(printf '%s' "$packs" | grep -c . || true)"
  if [ "${n:-0}" -eq 1 ]; then
    packdmg="$packs"
    say "$pack_log" "only one engine pack present ($(basename "$packdmg")); the installer's arch check decides"
  else
    echo "::error::no EZ-Convert-Engine-Pack-${host_arch}.dmg in the artifacts (found: ${n:-0} pack(s))" | tee -a "$pack_log"
    printf '%s\n' "$packs" | sed 's/^/    /' | tee -a "$pack_log"
    exit 1
  fi
fi
say "$pack_log" "selected engine pack: $(basename "$packdmg") (host $host_arch)"
verify_pin "$packdmg" "$pack_log"
pack_mnt="${RUNNER_TEMP:-/tmp}/pack-mnt"
rm -rf "$pack_mnt"; mkdir -p "$pack_mnt"
hdiutil attach -nobrowse -readonly -mountpoint "$pack_mnt" "$packdmg" >/dev/null
bash "$pack_mnt/Install Engine Pack.command" 2>&1 | tee -a "$pack_log"
hdiutil detach "$pack_mnt" >/dev/null

pack_root="$HOME/Library/Application Support/app.ezconvert.desktop"
say "$pack_log" "installed pack root:"
ls -la "$pack_root" | tee -a "$pack_log"
say "$pack_log" "engines:"
ls -la "$pack_root/engines" | tee -a "$pack_log"

echo "install-from-dmgs: OK (app + engine pack installed)"
