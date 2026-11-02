#!/usr/bin/env bash
# =============================================================================
# mac-engine-pack.sh — assemble, strip, sign and package the macOS engine pack.
#
# The macOS app bundle carries no engines: Tauri's macOS bundler signs
# `Contents/MacOS`, `Frameworks`, `Plugins`, `Helpers`, `XPCServices` and
# `Libraries`, but never `Contents/Resources`, and its resource copy flattens
# `.app` symlinks. So every engine ships here, in a separately versioned pack
# that the user installs once (docs/MACOS-PACKAGING.md §2).
#
# What it produces, in <out>:
#   engines/vips/{vips,vipsheader,lib/*.dylib,vips-modules-*}   libvips, relocated
#   engines/ffmpeg/{ffmpeg,ffprobe}                             LGPL build, no GPL
#   engines/soffice/{program,share,help,...}                    LibreOffice, stripped
#   licenses/                                                   notices + provenance
#   MANIFEST.sha256                                             every file, hashed
#   "EZ Convert Engine Pack.dmg"                                the payload
#
# Every engine is signed ad-hoc inside-out (nested objects first, outer bundle
# last). With a Developer ID the same order holds with `--options runtime
# --timestamp` — docs/MACOS-PACKAGING.md §4.
#
# USAGE
#   scripts/mac-engine-pack.sh --out <dir> [--arch arm64|x86_64]
#                              [--lo-url <url>] [--ffmpeg-url <url>]
#                              [--skip-ffmpeg-build]
#   Env: EZC_WORK (scratch dir), LO_VERSION, FFMPEG_VERSION
#
# EXIT: 0 packaged and verified, 1 any stage failed, 2 usage.
# =============================================================================
set -euo pipefail

ARCH="arm64"
OUT=""
LO_VERSION="${LO_VERSION:-26.8.2}"
FFMPEG_VERSION="${FFMPEG_VERSION:-8.1.1}"
LO_URL=""
FFMPEG_URL=""
SKIP_FFMPEG="${SKIP_FFMPEG:-0}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

while [ $# -gt 0 ]; do
  case "$1" in
    --out) OUT="${2:?--out needs a directory}"; shift 2 ;;
    --arch) ARCH="${2:?--arch needs arm64 or x86_64}"; shift 2 ;;
    --lo-url) LO_URL="${2:?}"; shift 2 ;;
    --ffmpeg-url) FFMPEG_URL="${2:?}"; shift 2 ;;
    --skip-ffmpeg-build) SKIP_FFMPEG=1; shift ;;
    -h|--help) sed -n '2,30p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

[ -n "$OUT" ] || { echo "usage: $0 --out <dir> [--arch arm64|x86_64]" >&2; exit 2; }
[ "$(uname -s)" = "Darwin" ] || { echo "this script only runs on macOS (needs hdiutil/codesign/otool)" >&2; exit 2; }

case "$ARCH" in
  arm64)  LO_ARCH_DIR="aarch64"; LO_FILE_ARCH="aarch64"; BREW_PREFIX="${BREW_PREFIX:-/opt/homebrew}" ;;
  x86_64) LO_ARCH_DIR="x86-64";  LO_FILE_ARCH="x86-64";  BREW_PREFIX="${BREW_PREFIX:-/usr/local}" ;;
  *) echo "unsupported --arch $ARCH" >&2; exit 2 ;;
esac

WORK="${EZC_WORK:-$(mktemp -d)}"
PACK="$WORK/pack"
STAGE="$WORK/stage"
mkdir -p "$PACK/engines" "$WORK/downloads" "$STAGE"

step() { printf '\n=== %s\n' "$*"; }
note() { printf '    %s\n' "$*"; }

# --------------------------------------------------------------------- 1. LO
step "1/8 LibreOffice $LO_VERSION ($ARCH)"
LO_DMG="$WORK/downloads/LibreOffice_${LO_VERSION}_MacOS_${LO_FILE_ARCH}.dmg"
if [ ! -f "$LO_DMG" ]; then
  if [ -z "$LO_URL" ]; then
    base="https://download.documentfoundation.org/libreoffice/stable/${LO_VERSION}/mac/${LO_ARCH_DIR}"
    # Pinned version only: the inventory row (docs/shipped-inventory.tsv) names
    # 26.8.2, so a different build must be a deliberate inventory change.
    LO_URL="$base/LibreOffice_${LO_VERSION}_MacOS_${LO_FILE_ARCH}.dmg"
  fi
  note "download $LO_URL"
  curl -fsSL --retry 3 -o "$LO_DMG" "$LO_URL"
fi
shasum -a 256 "$LO_DMG" | tee "$WORK/downloads/libreoffice.sha256"
note "$(du -h "$LO_DMG" | cut -f1) on disk"

MNT="$WORK/lo-mnt"
mkdir -p "$MNT"
hdiutil attach -nobrowse -readonly -mountpoint "$MNT" "$LO_DMG" >/dev/null
trap 'hdiutil detach "$MNT" >/dev/null 2>&1 || true' EXIT

LO_APP="$MNT/LibreOffice.app"
[ -d "$LO_APP" ] || { echo "no LibreOffice.app in the image: $(ls "$MNT")" >&2; exit 1; }

# Normalise the mac .app into the pack layout the engine layer already knows:
# engines/soffice/program (launchers + binaries) and engines/soffice/{share,...}
# from Contents/Resources. scripts/strip-engine-pack.py and the licence gate
# both expect that shape, and it is the same shape as the Windows/Linux packs.
#
# Measured layout of LibreOffice 26.8.2 MacOS aarch64 (CI run 41375947322, the
# `Contents` listing in pack.log): Contents/MacOS holds the executables
# (`soffice`, `unopkg`, `gengal`, `xpdfimport`, ...) plus symlinks into
# `../Resources` and `../Frameworks`; there is **no** `soffice.bin` and no
# `Contents/Resources/program`. So the flat layout is built by putting MacOS in
# `program/`, Resources' *contents* at the pack root, and Frameworks beside
# them — with `Resources` linked back to the root, because LibreOffice finds its
# data by a path relative to the executable: `../Resources/fundamentalrc`
# (cppuhelper/source/bootstrap.cxx), `../Resources/java/libreoffice.jar` and
# `../Frameworks` (desktop/scripts/unoinfo-mac.sh). Renaming MacOS to program/
# keeps `../Frameworks` correct; the `Resources` link keeps the other one.
CONTENTS="$LO_APP/Contents"
mkdir -p "$PACK/engines/soffice"
if [ -f "$CONTENTS/MacOS/soffice" ] || [ -L "$CONTENTS/MacOS/soffice" ]; then
  note "layout: program = Contents/MacOS, data = Contents/Resources (flat at the pack root)"
  ditto "$CONTENTS/MacOS" "$PACK/engines/soffice/program"
  ditto "$CONTENTS/Resources" "$PACK/engines/soffice"
  ln -sfn . "$PACK/engines/soffice/Resources"
  note "Resources -> . (so program/../Resources/<x> resolves)"
elif [ -d "$CONTENTS/Resources/program" ]; then
  note "layout: program = Contents/Resources/program"
  ditto "$CONTENTS/Resources" "$PACK/engines/soffice"
  ditto "$CONTENTS/Resources/program" "$PACK/engines/soffice/program"
else
  echo "unrecognised LibreOffice.app layout; Contents listing:" >&2
  ls -la "$CONTENTS" "$CONTENTS/MacOS" 2>&1 | head -40 >&2
  exit 1
fi
# The app is not just MacOS + Resources: the dylibs live in Contents/Frameworks
# (libmergedlo.dylib and the UNO libraries) and `../Frameworks` must resolve
# from program/, which it does once the directory is copied beside it.
for extra in Frameworks PlugIns Helpers Library; do
  if [ -d "$CONTENTS/$extra" ]; then
    ditto "$CONTENTS/$extra" "$PACK/engines/soffice/$extra"
    note "carried Contents/$extra ($(find "$CONTENTS/$extra" -type f | wc -l | tr -d ' ') files)"
  fi
done

# The launcher the engine layer spawns (ARC §6.2.1: console launcher, not .bin).
[ -x "$PACK/engines/soffice/program/soffice" ] || {
  echo "no executable engines/soffice/program/soffice after normalisation:" >&2
  ls -la "$PACK/engines/soffice/program" | head -30 >&2
  exit 1
}
hdiutil detach "$MNT" >/dev/null 2>&1 || true
trap - EXIT
note "$(find "$PACK/engines/soffice" -type f | wc -l | tr -d ' ') files, $(du -sh "$PACK/engines/soffice" | cut -f1)"

# ------------------------------------------------------------------- 2. strip
step "2/8 strip the content the licence decision removed (t_20fae747)"
python3 "$REPO_ROOT/scripts/strip-engine-pack.py" --pack "$PACK/engines/soffice" \
  --manifest "$WORK/downloads/strip-manifest.tsv"
note "$(wc -l < "$WORK/downloads/strip-manifest.tsv" | tr -d ' ') paths removed"

# -------------------------------------------------------------------- 3. vips
step "3/8 libvips ($ARCH) — Homebrew bottle, relocated to the pack"
if [ ! -x "$BREW_PREFIX/bin/brew" ] || ! brew list --versions vips >/dev/null 2>&1; then
  note "brew install vips"
  brew install vips
fi
VIPS_PREFIX="$(brew --prefix vips)"
VIPS_SRC="$VIPS_PREFIX"
[ -d "$VIPS_SRC/lib" ] || VIPS_SRC="$(brew --prefix)"
note "vips $("$VIPS_PREFIX/bin/vips" --version) from $VIPS_PREFIX"

VIPS_OUT="$PACK/engines/vips"
mkdir -p "$VIPS_OUT/lib"
cp "$VIPS_PREFIX/bin/vips" "$VIPS_PREFIX/bin/vipsheader" "$VIPS_OUT/"
# Runtime modules + codec plugins live beside the libraries in a bottle.
for dir in "$VIPS_PREFIX"/lib/vips-modules-*; do
  [ -d "$dir" ] && cp -R "$dir" "$VIPS_OUT/$(basename "$dir")"
done
[ -d "$VIPS_PREFIX/lib/libheif" ] && cp -R "$VIPS_PREFIX/lib/libheif" "$VIPS_OUT/libheif"

# Dependency closure: everything the binaries and modules link that is not a
# system library. Flattened into lib/ by basename, then rewritten to @rpath.
QUEUE="$WORK/vips-queue.txt"
SEEN="$WORK/vips-seen.txt"
: > "$QUEUE"; : > "$SEEN"
printf '%s\n' "$VIPS_OUT/vips" "$VIPS_OUT/vipsheader" >> "$QUEUE"
find "$VIPS_OUT" -name '*.so' -type f >> "$QUEUE" 2>/dev/null || true

deps_of() { otool -L "$1" 2>/dev/null | tail -n +2 | awk '{print $1}'; }

while :; do
  pending="$(sort -u "$QUEUE" | comm -23 - <(sort -u "$SEEN") || true)"
  [ -n "$pending" ] || break
  # `while read`, not `for file in $pending`: LibreOffice ships paths with
  # spaces, and word splitting turns each of those into two bogus filenames.
  while IFS= read -r file; do
    [ -n "$file" ] || continue
    printf '%s\n' "$file" >> "$SEEN"
    while IFS= read -r dep; do
      case "$dep" in
        /usr/lib/*|/System/*|@rpath/*|@loader_path/*|@executable_path/*) continue ;;
      esac
      base="$(basename "$dep")"
      [ -f "$VIPS_OUT/lib/$base" ] && continue
      [ -f "$dep" ] || { echo "missing dependency $dep (needed by $file)" >&2; exit 1; }
      cp "$dep" "$VIPS_OUT/lib/$base"
      printf '%s\n' "$VIPS_OUT/lib/$base" >> "$QUEUE"
    done < <(deps_of "$file")
  done <<< "$pending"
done
note "$(find "$VIPS_OUT/lib" -maxdepth 1 -name '*.dylib' | wc -l | tr -d ' ') dylibs in the closure"

# Rewrite install names: no /opt/homebrew (or /usr/local) path may survive, or
# the pack only runs on the build machine.
for lib in "$VIPS_OUT"/lib/*.dylib; do
  install_name_tool -id "@rpath/$(basename "$lib")" "$lib"
done
rewrite() { # <mach-o> [rpaths...]
  local file="$1"; shift
  while IFS= read -r dep; do
    case "$dep" in
      /usr/lib/*|/System/*|@rpath/*|@loader_path/*|@executable_path/*) continue ;;
    esac
    install_name_tool -change "$dep" "@rpath/$(basename "$dep")" "$file"
  done < <(deps_of "$file")
  for rpath in "$@"; do
    install_name_tool -add_rpath "$rpath" "$file" 2>/dev/null || true
  done
}
for lib in "$VIPS_OUT"/lib/*.dylib; do rewrite "$lib" "@loader_path"; done
rewrite "$VIPS_OUT/vips" "@executable_path/lib"
rewrite "$VIPS_OUT/vipsheader" "@executable_path/lib"
find "$VIPS_OUT" -name '*.so' -type f | while IFS= read -r module; do
  rewrite "$module" "@loader_path/../lib" "@loader_path"
done

leftover="$(find "$VIPS_OUT" -type f \( -name '*.dylib' -o -name 'vips' -o -name 'vipsheader' -o -name '*.so' \) -print0 \
  | xargs -0 otool -L 2>/dev/null | grep -E '/opt/homebrew|/usr/local/' | head -5 || true)"
if [ -n "$leftover" ]; then
  echo "relocation incomplete — absolute build paths still linked:" >&2
  printf '%s\n' "$leftover" >&2
  exit 1
fi
"$VIPS_OUT/vips" --version >/dev/null || { echo "relocated vips does not run" >&2; exit 1; }
note "relocated vips runs: $("$VIPS_OUT/vips" --version)"

# ------------------------------------------------------------------ 4. ffmpeg
step "4/8 ffmpeg $FFMPEG_VERSION ($ARCH) — built here, --disable-gpl"
if [ "$SKIP_FFMPEG" = "1" ] && [ -x "$WORK/downloads/ffmpeg/ffmpeg" ]; then
  note "reusing $WORK/downloads/ffmpeg"
else
  if [ ! -d "$WORK/downloads/ffmpeg-src" ]; then
    if [ -z "$FFMPEG_URL" ]; then
      FFMPEG_URL="https://ffmpeg.org/releases/ffmpeg-${FFMPEG_VERSION}.tar.xz"
    fi
    note "download $FFMPEG_URL"
    curl -fsSL --retry 3 -o "$WORK/downloads/ffmpeg.tar.xz" "$FFMPEG_URL"
    tar -xJf "$WORK/downloads/ffmpeg.tar.xz" -C "$WORK/downloads"
    mv "$WORK/downloads/ffmpeg-${FFMPEG_VERSION}" "$WORK/downloads/ffmpeg-src"
  fi
  # Only LGPL/BSD/permissive external libraries: libvpx (BSD), libaom (BSD),
  # opus (BSD), libvorbis (BSD), lame (LGPL-2.1). --disable-gpl is what the
  # licence gate asserts on the shipped binary; --disable-nonfree keeps the
  # build redistributable.
  note "configure"
  ( cd "$WORK/downloads/ffmpeg-src" && ./configure \
      --prefix="$WORK/downloads/ffmpeg" \
      --disable-gpl --disable-nonfree --disable-debug --disable-doc \
      --enable-libvpx --enable-libaom --enable-libopus --enable-libvorbis --enable-libmp3lame \
      --enable-videotoolbox --enable-audiotoolbox \
      >"$WORK/downloads/ffmpeg-configure.log" 2>&1 ) || {
      tail -40 "$WORK/downloads/ffmpeg-configure.log" >&2; exit 1; }
  note "build (this is the long step)"
  ( cd "$WORK/downloads/ffmpeg-src" && make -j"$(sysctl -n hw.ncpu)" ffmpeg ffprobe \
      >"$WORK/downloads/ffmpeg-build.log" 2>&1 && make install >>"$WORK/downloads/ffmpeg-build.log" 2>&1 ) || {
      tail -40 "$WORK/downloads/ffmpeg-build.log" >&2; exit 1; }
fi
mkdir -p "$PACK/engines/ffmpeg"
cp "$WORK/downloads/ffmpeg/bin/ffmpeg" "$WORK/downloads/ffmpeg/bin/ffprobe" "$PACK/engines/ffmpeg/"
"$PACK/engines/ffmpeg/ffmpeg" -buildconf | tee "$WORK/downloads/ffmpeg-buildconf.txt" | head -3
if "$PACK/engines/ffmpeg/ffmpeg" -buildconf | grep -qE -- '--enable-gpl|--enable-nonfree'; then
  echo "FATAL: ffmpeg build advertises --enable-gpl/--enable-nonfree" >&2
  exit 1
fi
note "ffmpeg $("$PACK/engines/ffmpeg/ffmpeg" -version | head -1)"

# ------------------------------------------------------------- 5. sign inside-out
step "5/8 sign inside-out (nested first, outer last)"
# Identity is ad-hoc for v1 and a Developer ID when one is funded: the order is
# what matters, so the same loop serves both (docs/MACOS-PACKAGING.md §4). The
# doc promises EZC_SIGN_IDENTITY/EZC_SIGN_OPTIONS here — honour it.
SIGN_IDENTITY="${EZC_SIGN_IDENTITY:--}"
SIGN_OPTIONS="${EZC_SIGN_OPTIONS:-}"
if [ "$SIGN_IDENTITY" = "-" ]; then SIGN_OPTIONS="--timestamp=none"; fi
note "identity: $SIGN_IDENTITY ${SIGN_OPTIONS}"
sign() { local out
  # shellcheck disable=SC2086
  if ! out="$(codesign --force --sign "$SIGN_IDENTITY" $SIGN_OPTIONS "$1" 2>&1)"; then
    echo "codesign failed: $1" >&2
    printf '%s\n' "$out" | sed 's/^/    /' >&2
    return 1
  fi; }
# 5a nested Mach-O files, deepest path first. BSD `file` emits one line per
# architecture of a universal binary ("<path> (for architecture arm64): ..."):
# those are not paths and codesign fails on them, so the suffix is stripped and
# the list de-duplicated before anything is signed.
find "$PACK/engines" -type f -print0 \
  | xargs -0 file 2>/dev/null | grep -E 'Mach-O' \
  | sed -E 's/ \(for architecture [^)]*\)$//' | cut -d: -f1 | sort -u \
  | awk -F/ '{print NF" "$0}' | sort -rn -k1,1 | cut -d' ' -f2- > "$WORK/macho.txt" || true
[ -s "$WORK/macho.txt" ] || { echo "no Mach-O files found under $PACK/engines" >&2; exit 1; }
while IFS= read -r f; do sign "$f"; done < "$WORK/macho.txt"
note "$(wc -l < "$WORK/macho.txt" | tr -d ' ') Mach-O files signed"
for probe in "$PACK/engines/vips/vips" "$PACK/engines/ffmpeg/ffmpeg" "$PACK/engines/soffice/program/soffice.bin"; do
  [ -f "$probe" ] || { echo "missing probe $probe" >&2; exit 1; }
  codesign --verify "$probe" || { echo "unsigned nested object: $probe" >&2; exit 1; }
done

# -------------------------------------------------------------- 6. licence scan
step "6/8 licence gate over the assembled pack (fail-closed)"
bash "$REPO_ROOT/scripts/dist-license-gate.sh" --scan "$PACK" | tee "$WORK/downloads/licence-scan.txt" | tail -5

# -------------------------------------------------------------------- 7. smoke
step "7/8 smoke: every engine must run and convert a real file"
FIX="$REPO_ROOT/fixtures"
mkdir -p "$WORK/smoke"
"$PACK/engines/vips/vips" copy "$FIX/sample.png" "$WORK/smoke/image.jpg[Q=85]" || { echo "vips convert failed" >&2; exit 1; }
[ -s "$WORK/smoke/image.jpg" ] || { echo "vips produced no output" >&2; exit 1; }
note "image png->jpg $(stat -f%z "$WORK/smoke/image.jpg") B"

"$PACK/engines/ffmpeg/ffmpeg" -nostdin -v error -y -i "$FIX/sample.wav" -codec:a libmp3lame -q:a 2 "$WORK/smoke/audio.mp3" || { echo "ffmpeg audio failed" >&2; exit 1; }
[ -s "$WORK/smoke/audio.mp3" ] || { echo "ffmpeg produced no audio output" >&2; exit 1; }
note "audio wav->mp3 $(stat -f%z "$WORK/smoke/audio.mp3") B"

"$PACK/engines/soffice/program/soffice" --headless --norestore --terminate_after_init >/dev/null 2>&1 || true
"$PACK/engines/soffice/program/soffice" --headless --norestore \
  -env:UserInstallation="file://$WORK/smoke/lo-profile" \
  --convert-to pdf --outdir "$WORK/smoke" "$FIX/sample.docx" >/dev/null 2>&1 || true
[ -s "$WORK/smoke/sample.pdf" ] || { echo "LibreOffice produced no PDF (checking its own words)" >&2
  "$PACK/engines/soffice/program/soffice" --headless --norestore -env:UserInstallation="file://$WORK/smoke/lo-profile2" --convert-to pdf --outdir "$WORK/smoke" "$FIX/sample.docx" 2>&1 | tail -20 >&2
  exit 1; }
note "document docx->pdf $(stat -f%z "$WORK/smoke/sample.pdf") B"

# --------------------------------------------------------------- 8. package it
step "8/8 package the engine pack DMG"
mkdir -p "$PACK/licenses"
cp "$REPO_ROOT/docs/licenses/THIRD-PARTY-NOTICES.md" "$PACK/licenses/" 2>/dev/null || true
cp "$WORK/downloads/ffmpeg-buildconf.txt" "$PACK/licenses/ffmpeg-buildconf.txt" || true
cp "$WORK/downloads/libreoffice.sha256" "$PACK/licenses/libreoffice.sha256" || true
cp "$WORK/downloads/strip-manifest.tsv" "$PACK/licenses/strip-manifest.tsv" || true
cp "$WORK/downloads/licence-scan.txt" "$PACK/licenses/licence-scan.txt" || true
if command -v brew >/dev/null; then
  brew info --json=v2 vips > "$PACK/licenses/vips-bottle.json" 2>/dev/null || true
fi
cp "$REPO_ROOT/packaging/macos/install-engine-pack.command" "$STAGE/Install Engine Pack.command" && chmod +x "$STAGE/Install Engine Pack.command"
cp "$REPO_ROOT/packaging/macos/README-pack.txt" "$STAGE/README.txt"

( cd "$PACK" && find . -type f ! -name MANIFEST.sha256 -print0 > "$WORK/filelist.txt" \
  && xargs -0 shasum -a 256 < "$WORK/filelist.txt" > MANIFEST.sha256 )
note "manifest: $(wc -l < "$PACK/MANIFEST.sha256" | tr -d ' ') files"

ditto "$PACK" "$STAGE/engines-root"
mkdir -p "$STAGE/payload"
mv "$STAGE/engines-root" "$STAGE/payload/engines"
cp "$PACK/MANIFEST.sha256" "$STAGE/payload/MANIFEST.sha256"

# The gate then runs over the DMG root as well, not only the pack tree: the
# three files the image adds over $PACK (readme, installer, manifest copy) must
# attribute too — otherwise a fail-closed gate says nothing about what a user
# actually mounts. Output lands in pack.log on the macos-status branch.
ROOTCHECK="$WORK/rootcheck"
mkdir -p "$ROOTCHECK"
cp "$STAGE/README.txt" "$STAGE/Install Engine Pack.command" "$STAGE/payload/MANIFEST.sha256" "$ROOTCHECK/"
note "licence gate over the DMG root files"
bash "$REPO_ROOT/scripts/dist-license-gate.sh" --scan "$ROOTCHECK" | tail -3

mkdir -p "$OUT"
DMG="$OUT/EZ-Convert-Engine-Pack-${ARCH}.dmg"
rm -f "$DMG"
hdiutil create -volname "EZ Convert Engine Pack" -srcfolder "$STAGE" -fs HFS+ -format UDZO -ov "$DMG" >/dev/null
# The DMG is signed last, after every object inside it (docs/MACOS-PACKAGING §3
# steps 5-6). Not `|| true`: an unsigned DMG is a silent regression the user
# only meets at Gatekeeper.
# shellcheck disable=SC2086
codesign --force --sign "$SIGN_IDENTITY" $SIGN_OPTIONS "$DMG" || { echo "DMG signing failed: $DMG" >&2; exit 1; }
shasum -a 256 "$DMG" | tee "$OUT/EZ-Convert-Engine-Pack-${ARCH}.dmg.sha256"
note "$(du -h "$DMG" | cut -f1) → $DMG"
printf 'engine pack: OK arch=%s dmg=%s bytes=%s\n' "$ARCH" "$DMG" "$(stat -f%z "$DMG")"
