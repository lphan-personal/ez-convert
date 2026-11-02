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
#   MANIFEST.json                                               version + engine set
#   "EZ Convert Engine Pack.dmg"                                the payload
#   <dmg>.sha256, <dmg>.manifest.json                           published pins
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
  # TDF's download directory and its file name disagree, measured from the
  # 26.8.2 listing (https://download.documentfoundation.org/libreoffice/stable/26.8.2/mac/):
  #   mac/aarch64/LibreOffice_26.8.2_MacOS_aarch64.dmg   (dir and name agree)
  #   mac/x86_64/LibreOffice_26.8.2_MacOS_x86-64.dmg     (dir x86_64, name x86-64)
  # The never-run x86_64 path had `x86-64` for both, which 404s on the
  # directory — it would have failed after the ~5-minute download wait, not
  # before it. LO_ARCH_DIR is the URL directory; LO_FILE_ARCH is the file name.
  arm64)  LO_ARCH_DIR="aarch64"; LO_FILE_ARCH="aarch64"; BREW_PREFIX="${BREW_PREFIX:-/opt/homebrew}" ;;
  x86_64) LO_ARCH_DIR="x86_64";  LO_FILE_ARCH="x86-64";  BREW_PREFIX="${BREW_PREFIX:-/usr/local}" ;;
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

# The pack keeps the .app exactly as TDF builds it. LibreOffice resolves its data
# relative to the executable *inside* the bundle (`@executable_path/../Frameworks`,
# `../Resources`), and a flattened copy — program/ = Contents/MacOS, Resources
# content at the pack root, `Resources -> .` — starts, then aborts with a UNO
# DeploymentException on the first conversion (CI run 41384142116: png->jpg and
# wav->mp3 converted fine, soffice died with `Abort trap: 6` and an empty log).
# Shipping the bundle intact is also what this card asked for: an external pack
# that preserves the `.app` and its symlinks.
#
# Measured layout of LibreOffice 26.8.2 MacOS aarch64: Contents/MacOS holds the
# executables (`soffice`, `unopkg`, `gengal`, `xpdfimport`, ...) plus symlinks into
# `../Resources` and `../Frameworks`; there is no `soffice.bin`; data lives in
# Contents/Resources, libraries in Contents/Frameworks.
mkdir -p "$PACK/engines/soffice"
note "layout: LibreOffice.app preserved (Contents/{MacOS,Resources,Frameworks,...})"
ditto "$LO_APP" "$PACK/engines/soffice/LibreOffice.app"

# Compatibility symlink, not a copy: `engines/soffice/program/soffice` is the path
# the Windows/Linux packs and the tooling use, and it must keep resolving. The
# app itself prefers the real bundle path on macOS (engine::proc::from_root), so
# LibreOffice still runs from inside the bundle.
ln -sfn "LibreOffice.app/Contents/MacOS" "$PACK/engines/soffice/program"

# The launcher the engine layer spawns (ARC §6.2.1: console launcher, not .bin).
[ -x "$PACK/engines/soffice/LibreOffice.app/Contents/MacOS/soffice" ] || {
  echo "no executable LibreOffice.app/Contents/MacOS/soffice after packaging:" >&2
  ls -la "$PACK/engines/soffice/LibreOffice.app/Contents/MacOS" 2>&1 | head -30 >&2
  exit 1
}
note "symlinks preserved: $(find "$PACK/engines/soffice/LibreOffice.app" -type l | wc -l | tr -d ' ') in the bundle"
hdiutil detach "$MNT" >/dev/null 2>&1 || true
trap - EXIT
note "$(find "$PACK/engines/soffice" -type f | wc -l | tr -d ' ') files, $(du -sh "$PACK/engines/soffice" | cut -f1)"

# ------------------------------------------------------------------- 2. strip
step "2/8 strip the content the licence decision removed (t_20fae747)"
# The strip runs against the bundle's Resources tree (that is where share/,
# gallery and help live in the .app); --program only relaxes the tool's
# "is this really a LibreOffice tree" guard.
LO_RES="$PACK/engines/soffice/LibreOffice.app/Contents/Resources"
LO_BIN="$PACK/engines/soffice/LibreOffice.app/Contents/MacOS"
python3 "$REPO_ROOT/scripts/strip-engine-pack.py" --pack "$LO_RES" --program "$LO_BIN/soffice" \
  --manifest "$WORK/downloads/strip-manifest.tsv"
note "$(wc -l < "$WORK/downloads/strip-manifest.tsv" | tr -d ' ') paths removed"
# The gate's per-group rows are written against the Windows/Linux pack paths
# (`share/fonts/truetype/DavidCLM-*`), while the macOS pack nests LibreOffice
# under `engines/soffice/LibreOffice.app/Contents/Resources`, so the
# failure-closed "not-shipped" check cannot see a survivor here. Assert the
# removal directly instead: a strip that silently did nothing must not pass as a
# licence decision.
for gone in "$LO_RES/share/gallery" "$LO_RES/gallery" "$LO_RES/help"; do
  # The directory itself may survive the removal (the strip removes files, not
  # empty parents); what must be gone is every file under it.
  if [ -d "$gone" ] && [ -n "$(find "$gone" -type f -print -quit 2>/dev/null)" ]; then
    echo "strip incomplete: files still under $gone" >&2
    exit 1
  fi
done
survivor="$(find "$LO_RES" \( -name 'DavidCLM-*' -o -name 'FrankRuehlCLM-*' \
  -o -name 'MiriamCLM-*' -o -name 'MiriamMonoCLM-*' -o -name 'NachlieliCLM-*' \) -print -quit)"
[ -z "$survivor" ] || { echo "strip incomplete: GPL-2.0 Culmus font still shipped: $survivor" >&2; exit 1; }
note "strip verified: gallery, help and the five Culmus families are gone"

# --------------------------------------------------- 2b. LibreOffice boots early
# LibreOffice is the biggest and least predictable dependency, and the ffmpeg
# build after this takes minutes of billed macOS time. Prove LO boots and converts
# *here*, right after the layout is built, and print what it is looking at when it
# does not. (Run 41384142116 got through the licence gate and both media engines
# and only then hit `Abort trap: 6 / DeploymentException` from soffice.)
step "2b/8 smoke LibreOffice before the long engines are built"
LO_PROFILE="file://$WORK/smoke/lo-early"
mkdir -p "$WORK/smoke"
"$LO_BIN/soffice" --headless --norestore \
  -env:UserInstallation="$LO_PROFILE" --convert-to pdf --outdir "$WORK/smoke" \
  "$REPO_ROOT/fixtures/sample.docx" >"$WORK/smoke/soffice-early.log" 2>&1 || true
if [ ! -s "$WORK/smoke/sample.pdf" ]; then
  echo "LibreOffice did not produce a PDF from the pack. What it said:" >&2
  tail -20 "$WORK/smoke/soffice-early.log" >&2
  echo "--- engines/soffice layout ---" >&2
  ls -la "$PACK/engines/soffice" >&2
  echo "--- program/ (first 20) ---" >&2
  ls -la "$LO_BIN" 2>&1 | head -20 >&2
  echo "--- probes ---" >&2
  for probe in program/soffice program/soffice.bin fundamentalrc Resources \
               registry/main.xcd share/registry/main.xcd program/services; do
    p="$PACK/engines/soffice/$probe"
    if [ -L "$p" ]; then printf '  %-24s symlink -> %s\n' "$probe" "$(readlink "$p")" >&2
    elif [ -e "$p" ]; then printf '  %-24s present\n' "$probe" >&2
    else printf '  %-24s MISSING\n' "$probe" >&2; fi
  done
  echo "--- what the launcher links ---" >&2
  otool -L "$LO_BIN/soffice" 2>&1 | head -8 >&2 || true
  echo "--- bootstrap files that mention BRAND_BASE_DIR ---" >&2
  grep -rl "BRAND_BASE_DIR" "$PACK/engines/soffice" 2>/dev/null | head -5 >&2 || true
  exit 1
fi
note "LibreOffice converts from the pack: $(stat -f%z "$WORK/smoke/sample.pdf") B pdf"

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
# `vips --version` prints `vips-8.18.0` (one token), so the version is the number
# in that token — `awk '{print $2}'` is empty and the manifest then refuses the
# engine (run 41386249708 failed at step 8 on `--engine vips=`).
VIPS_VERSION="$("$VIPS_PREFIX/bin/vips" --version 2>/dev/null | sed -E 's/^[^0-9]*([0-9][0-9.]*).*/\1/' | head -1)"
[ -n "$VIPS_VERSION" ] || { echo "cannot read the vips version from $("$VIPS_PREFIX/bin/vips" --version 2>&1)" >&2; exit 1; }

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

# Every Mach-O in the pack, found by content rather than by extension: the vips
# *modules* are not all `.so` on macOS (run 41376620641), and BSD `file` adds one
# parenthetical line per architecture of a universal binary.
macho_list() {
  find "$VIPS_OUT" -type f -print0 | xargs -0 file 2>/dev/null | grep -E 'Mach-O' \
    | sed -E 's/ \(for architecture [^)]*\)//g' | cut -d: -f1 | sort -u
}

# Where a linked dependency actually lives on this machine. Homebrew bottles mix
# absolute paths with @rpath references (openexr's libOpenEXR refers to
# @rpath/libIlmThread-3_5.34.dylib), so an @rpath dep is looked up in the source
# prefix — otherwise it is never staged and the relocated libvips aborts with
# "Library not loaded" (run 41378731944).
BREW_LIB="$(brew --prefix)/lib"
source_dep() { # <dep> -> absolute source path, empty if unresolvable
  local dep="$1" base dir
  case "$dep" in
    @rpath/*|@loader_path/*|@executable_path/*) base="$(basename "$dep")" ;;
    /*) printf '%s' "$dep"; return 0 ;;
    *) return 0 ;;
  esac
  for dir in "$VIPS_PREFIX/lib" "$VIPS_SRC/lib" "$BREW_LIB"; do
    if [ -f "$dir/$base" ]; then printf '%s' "$dir/$base"; return 0; fi
  done
}

# Dependency closure: everything the pack links that is not a system library.
# Iterated to a fixed point over every Mach-O in the tree, so a dependency two
# hops away (@rpath -> sibling -> third) is not missed.
for _pass in 1 2 3 4 5 6; do
  added=0
  while IFS= read -r macho; do
    while IFS= read -r dep; do
      case "$dep" in
        /usr/lib/*|/System/*|@loader_path/*|@executable_path/*) continue ;;
      esac
      base="$(basename "$dep")"
      [ -f "$VIPS_OUT/lib/$base" ] && continue
      src="$(source_dep "$dep")"
      if [ -z "$src" ] || [ ! -f "$src" ]; then
        echo "unresolvable dependency: $dep (linked by $macho)" >&2
        exit 1
      fi
      cp "$src" "$VIPS_OUT/lib/$base"
      added=$((added + 1))
    done < <(deps_of "$macho")
  done < <(macho_list)
  [ "$added" -eq 0 ] && break
done
note "$(find "$VIPS_OUT/lib" -maxdepth 1 -type f | wc -l | tr -d ' ') dylibs in the closure"

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

macho_list > "$WORK/vips-macho.txt"
while IFS= read -r macho; do
  rewrite "$macho" "@loader_path" "@loader_path/../lib" "@executable_path/lib"
done < "$WORK/vips-macho.txt"
note "$(wc -l < "$WORK/vips-macho.txt" | tr -d ' ') Mach-O objects re-pointed at @rpath"

# Post-conditions, per file and reported in full (a macOS run is ~47 billed
# minutes, so one run must list every offender, not the first one):
#  (a) no absolute build path is still linked;
#  (b) every non-system, non-absolute dependency resolves to a file that is
#      actually inside the pack.
leftover=""
missing=""
while IFS= read -r macho; do
  while IFS= read -r dep; do
    case "$dep" in
      /opt/homebrew/*|/usr/local/*)
        leftover="${leftover}${macho}: ${dep}"$'\n' ; continue ;;
      /usr/lib/*|/System/*)
        continue ;;
      @rpath/*)
        base="$(basename "$dep")"
        [ -f "$VIPS_OUT/lib/$base" ] || [ -f "$(dirname "$macho")/$base" ] \
          || missing="${missing}${macho}: ${dep}"$'\n'
        continue ;;
      @loader_path/*)
        rel="${dep#@loader_path/}"
        [ -e "$(dirname "$macho")/$rel" ] || missing="${missing}${macho}: ${dep}"$'\n'
        continue ;;
      *)
        [ -f "$dep" ] || missing="${missing}${macho}: ${dep}"$'\n' ;;
    esac
  done < <(deps_of "$macho")
done < "$WORK/vips-macho.txt"
if [ -n "$leftover" ]; then
  echo "relocation incomplete — absolute build paths still linked:" >&2
  printf '%s\n' "$leftover" >&2
  exit 1
fi
if [ -n "$missing" ]; then
  echo "dependency closure incomplete — these references do not resolve inside the pack:" >&2
  printf '%s\n' "$missing" >&2
  exit 1
fi
note "dependency gate: every reference in $(wc -l < "$WORK/vips-macho.txt" | tr -d ' ') Mach-O objects resolves inside the pack"
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
  # FFmpeg 8.1 probes libmp3lame with a plain `require` — configure line 7325 of
  # n8.1.1:
  #   enabled libmp3lame && require "libmp3lame >= 3.98.3" lame/lame.h \
  #                                    lame_set_VBR_quality -lmp3lame $libm_extralibs
  # That is check_lib: no pkg-config, no cflags. So a .pc file cannot fix it — run
  # 41382615266 had a correct libmp3lame.pc on PKG_CONFIG_PATH and still died on
  # `fatal error: 'lame/lame.h' file not found` — and clang does not search
  # /opt/homebrew/include. The keg's include root has to be on the compile line.
  #
  # Measured from the 4.0 bottle itself (arm64_tahoe): include/lame/lame.h exists,
  # the LAME_*_VERSION macros are gone, and the keg's own lame.pc declares
  # `Cflags: -I${includedir}/lame` — one level too deep for `#include <lame/lame.h>`.
  # The include root below is the header's grandparent, so it resolves by
  # construction rather than by luck.
  LAME_PREFIX="$(brew --prefix lame)"
  LAME_HDR="$(find "$LAME_PREFIX/include" -name lame.h -print -quit 2>/dev/null || true)"
  if [ -z "$LAME_HDR" ]; then
    echo "no lame.h under $LAME_PREFIX/include — the keg ships no headers to compile against:" >&2
    ls -R "$LAME_PREFIX/include" 2>&1 | head -20 >&2
    exit 1
  fi
  LAME_INCLUDE_ROOT="$(dirname "$(dirname "$LAME_HDR")")"
  note "lame $(brew list --versions lame | awk '{print $2}') header: $LAME_HDR"
  note "ffmpeg compile line gets -I$LAME_INCLUDE_ROOT (--extra-cflags)"
  EXTRA_CFLAGS="--extra-cflags=-I$LAME_INCLUDE_ROOT"
  # ...and the library path: lame is keg-only, so `ld: library 'mp3lame' not
  # found` is what run 41383311843 hit on the link half of the same probe.
  EXTRA_LDFLAGS="--extra-ldflags=-L$LAME_PREFIX/lib"
  ( cd "$WORK/downloads/ffmpeg-src" && ./configure \
      --prefix="$WORK/downloads/ffmpeg" \
      "$EXTRA_CFLAGS" "$EXTRA_LDFLAGS" \
      --disable-gpl --disable-nonfree --disable-debug --disable-doc \
      --enable-libvpx --enable-libaom --enable-libopus --enable-libvorbis --enable-libmp3lame \
      --enable-videotoolbox --enable-audiotoolbox \
      >"$WORK/downloads/ffmpeg-configure.log" 2>&1 ) || {
      tail -40 "$WORK/downloads/ffmpeg-configure.log" >&2
      # configure's last 40 lines name the failed check but not why it failed;
      # config.log has the compiler/pkg-config probe. Print it in the same run —
      # a macOS run is ~47 billed minutes, so "run it again to learn more" is not
      # an acceptable diagnostic loop (run 41379802059 needed exactly this).
      if [ -f "$WORK/downloads/ffmpeg-src/ffbuild/config.log" ]; then
        echo "--- ffbuild/config.log around the failing probe ---" >&2
        grep -n -i -B3 -A25 -E 'libmp3lame|lame >=|ERROR' "$WORK/downloads/ffmpeg-src/ffbuild/config.log" | tail -80 >&2 || true
      fi
      exit 1; }
  note "build (this is the long step)"
  ( cd "$WORK/downloads/ffmpeg-src" && make -j"$(sysctl -n hw.ncpu)" ffmpeg ffprobe \
      >"$WORK/downloads/ffmpeg-build.log" 2>&1 && make install >>"$WORK/downloads/ffmpeg-build.log" 2>&1 ) || {
      tail -40 "$WORK/downloads/ffmpeg-build.log" >&2; exit 1; }
fi
mkdir -p "$PACK/engines/ffmpeg"
cp "$WORK/downloads/ffmpeg/bin/ffmpeg" "$WORK/downloads/ffmpeg/bin/ffprobe" "$PACK/engines/ffmpeg/"

# ffmpeg's external codecs come from Homebrew kegs, so both binaries link dylibs
# by absolute path (/opt/homebrew/opt/lame/lib/..., libvpx, aom, opus, vorbis,
# mpg123). Left alone, the pack would run only on the machine that built it.
# Stage the closure into engines/ffmpeg/lib and re-point every reference at
# @rpath, the same way the vips tree above is made relocatable — same helpers,
# same post-condition: no build path may survive.
FF_OUT="$PACK/engines/ffmpeg"
mkdir -p "$FF_OUT/lib"
FF_PROVENANCE="$WORK/downloads/ffmpeg-dylibs.tsv"
printf '# staged dylib\tcopied from\n' > "$FF_PROVENANCE"

ff_machos() {
  find "$FF_OUT" -type f -print0 | xargs -0 file 2>/dev/null | grep -E 'Mach-O' \
    | sed -E 's/ \(for architecture [^)]*\)//g' | cut -d: -f1 | sort -u
}

for _pass in 1 2 3 4 5 6; do
  added=0
  while IFS= read -r macho; do
    while IFS= read -r dep; do
      case "$dep" in
        /usr/lib/*|/System/*|@loader_path/*|@executable_path/*|@rpath/*) continue ;;
      esac
      base="$(basename "$dep")"
      [ -f "$FF_OUT/lib/$base" ] && continue
      if [ ! -f "$dep" ]; then
        echo "ffmpeg dependency not present on the build machine: $dep" >&2
        echo "  linked by ${macho#"$PACK"/}" >&2
        exit 1
      fi
      cp "$dep" "$FF_OUT/lib/$base"
      chmod u+w "$FF_OUT/lib/$base"
      printf '%s\t%s\n' "$base" "$dep" >> "$FF_PROVENANCE"
      added=$((added + 1))
    done < <(deps_of "$macho")
  done < <(ff_machos)
  [ "$added" -eq 0 ] && break
done
note "$(find "$FF_OUT/lib" -type f | wc -l | tr -d ' ') dylibs staged into engines/ffmpeg/lib"

for lib in "$FF_OUT"/lib/*.dylib; do
  [ -e "$lib" ] || continue
  install_name_tool -id "@rpath/$(basename "$lib")" "$lib"
done
while IFS= read -r macho; do
  rewrite "$macho" "@loader_path/lib" "@loader_path" "@executable_path/lib"
done < <(ff_machos)

ff_leftover=""
ff_missing=""
while IFS= read -r macho; do
  while IFS= read -r dep; do
    case "$dep" in
      /opt/homebrew/*|/usr/local/*)
        ff_leftover="${ff_leftover}${macho#"$PACK"/}: ${dep}"$'\n' ; continue ;;
      /usr/lib/*|/System/*) continue ;;
      @rpath/*)
        base="$(basename "$dep")"
        [ -f "$FF_OUT/lib/$base" ] || [ -f "$(dirname "$macho")/$base" ] \
          || ff_missing="${ff_missing}${macho#"$PACK"/}: ${dep}"$'\n'
        continue ;;
      @loader_path/*)
        rel="${dep#@loader_path/}"
        [ -e "$(dirname "$macho")/$rel" ] || ff_missing="${ff_missing}${macho#"$PACK"/}: ${dep}"$'\n'
        continue ;;
      *) [ -f "$dep" ] || ff_missing="${ff_missing}${macho#"$PACK"/}: ${dep}"$'\n' ;;
    esac
  done < <(deps_of "$macho")
done < <(ff_machos)
[ -z "$ff_leftover" ] || { echo "ffmpeg relocation incomplete — absolute build paths still linked:" >&2; printf '%s' "$ff_leftover" >&2; exit 1; }
[ -z "$ff_missing" ] || { echo "ffmpeg dependency closure incomplete:" >&2; printf '%s' "$ff_missing" >&2; exit 1; }
note "ffmpeg closure: every reference resolves inside the pack"

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
# those are not paths and codesign fails on them, so the parenthetical is
# removed wherever it appears (end-anchored matching misses it — the line
# continues after the colon) and the list de-duplicated.
find "$PACK/engines" -type f -print0 \
  | xargs -0 file 2>/dev/null | grep -E 'Mach-O' \
  | sed -E 's/ \(for architecture [^)]*\)//g' | cut -d: -f1 | sort -u \
  | awk -F/ '{print NF" "$0}' | sort -rn -k1,1 | cut -d' ' -f2- > "$WORK/macho.txt" || true
[ -s "$WORK/macho.txt" ] || { echo "no Mach-O files found under $PACK/engines" >&2; exit 1; }
while IFS= read -r f; do sign "$f"; done < "$WORK/macho.txt"
note "$(wc -l < "$WORK/macho.txt" | tr -d ' ') Mach-O files signed"
# Nested bundles after their contents, outer last (docs/MACOS-PACKAGING.md §3).
# The LibreOffice .app keeps its own seal this way, so `codesign --verify` on the
# bundle is meaningful for anything that later inspects it.
find "$PACK/engines" -mindepth 1 -type d \( -name '*.framework' -o -name '*.app' -o -name '*.appex' \) -print 2>/dev/null \
  | awk -F/ '{print NF" "$0}' | sort -rn -k1,1 | cut -d' ' -f2- > "$WORK/bundles.txt" || true
while IFS= read -r bundle; do
  [ -n "$bundle" ] || continue
  sign "$bundle"
done < "$WORK/bundles.txt"
[ -s "$WORK/bundles.txt" ] && note "$(wc -l < "$WORK/bundles.txt" | tr -d ' ') nested bundles signed"
# The launcher the app spawns is `program/soffice` (ARC §6.2.1: the console
# launcher, never `soffice.bin`). The Windows/Linux packs have a `soffice.bin`;
# this one does not — measured on the 26.8.2 MacOS build, where Contents/MacOS
# holds the launchers and there is no `program/soffice.bin` at all.
for probe in "$PACK/engines/vips/vips" "$PACK/engines/ffmpeg/ffmpeg" "$LO_BIN/soffice"; do
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

"$LO_BIN/soffice" --headless --norestore --terminate_after_init >/dev/null 2>&1 || true
"$LO_BIN/soffice" --headless --norestore \
  -env:UserInstallation="file://$WORK/smoke/lo-profile" \
  --convert-to pdf --outdir "$WORK/smoke" "$FIX/sample.docx" >/dev/null 2>&1 || true
[ -s "$WORK/smoke/sample.pdf" ] || { echo "LibreOffice produced no PDF (checking its own words)" >&2
  "$LO_BIN/soffice" --headless --norestore -env:UserInstallation="file://$WORK/smoke/lo-profile2" --convert-to pdf --outdir "$WORK/smoke" "$FIX/sample.docx" 2>&1 | tail -20 >&2
  exit 1; }
note "document docx->pdf $(stat -f%z "$WORK/smoke/sample.pdf") B"

# --------------------------------------------------------------- 8. package it
step "8/8 package the engine pack DMG"
mkdir -p "$PACK/licenses"
cp "$REPO_ROOT/docs/licenses/THIRD-PARTY-NOTICES.md" "$PACK/licenses/" 2>/dev/null || true
cp "$WORK/downloads/ffmpeg-buildconf.txt" "$PACK/licenses/ffmpeg-buildconf.txt" || true
cp "$WORK/downloads/libreoffice.sha256" "$PACK/licenses/libreoffice.sha256" || true
cp "$WORK/downloads/ffmpeg-dylibs.tsv" "$PACK/licenses/ffmpeg-dylibs.tsv" || true
cp "$WORK/downloads/strip-manifest.tsv" "$PACK/licenses/strip-manifest.tsv" || true
cp "$WORK/downloads/licence-scan.txt" "$PACK/licenses/licence-scan.txt" || true
if command -v brew >/dev/null; then
  brew info --json=v2 vips > "$PACK/licenses/vips-bottle.json" 2>/dev/null || true
fi
cp "$REPO_ROOT/packaging/macos/install-engine-pack.command" "$STAGE/Install Engine Pack.command" && chmod +x "$STAGE/Install Engine Pack.command"
cp "$REPO_ROOT/packaging/macos/README-pack.txt" "$STAGE/README.txt"

# The version + checksum manifest (scripts/pack-manifest.py). MANIFEST.sha256
# answers "are these the bytes that were built?"; MANIFEST.json answers "which
# pack is this, which engines at which versions, which arch, and where does it
# install?" — the two cross-reference each other (see that script's header).
# `seal` writes both (it hashes the tree once; hashing it here first would hash
# ~700 MB twice for nothing). The engine versions are the ones this run used.
LO_SHA="$(awk '{print $1}' "$WORK/downloads/libreoffice.sha256")"
python3 "$REPO_ROOT/scripts/pack-manifest.py" seal --pack "$PACK" --arch "$ARCH" \
  --pack-version "${PACK_VERSION:-1.0.0}" \
  --engine "soffice=$LO_VERSION" --engine "ffmpeg=$FFMPEG_VERSION" --engine "vips=$VIPS_VERSION" \
  --source "libreoffice-dmg=sha256:$LO_SHA" \
  --source "ffmpeg-buildconf=file:$WORK/downloads/ffmpeg-buildconf.txt"
python3 "$REPO_ROOT/scripts/pack-manifest.py" verify --pack "$PACK" --expect-arch "$ARCH"

# The payload IS the pack root: engines/, licenses/ and MANIFEST.sha256 at the
# top of it. (Earlier this nested $PACK under payload/engines, which installed
# a second "engines" level inside app.ezconvert.desktop/engines and would have
# left the app unable to find any engine — the layout the installer and
# adopt_pack agree on is the one below.)
mkdir -p "$STAGE/payload"
ditto "$PACK" "$STAGE/payload"

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
# A DMG cannot contain its own digest, so the published bytes are pinned beside
# it: name, bytes, sha256, the pack id/version/arch, and the digest of the
# MANIFEST.json inside. That is the file a download is checked against before
# anything is mounted.
python3 "$REPO_ROOT/scripts/pack-manifest.py" seal-artifact \
  --dmg "$DMG" --pack-manifest "$PACK/MANIFEST.json"
note "$(du -h "$DMG" | cut -f1) → $DMG"

# 8b. Verify the artifact the way it will be used: mount it read-only, check the
# sidecar digest, then the manifest pair, the symlink inventory, every Mach-O
# signature and all three engines running from inside the image. A pack that
# builds but does not survive `hdiutil create` is not a shipped pack.
step "8b/8 verify the artifact as a user receives it"
bash "$REPO_ROOT/scripts/verify-engine-pack.sh" --dmg "$DMG" --expect-arch "$ARCH" \
  | tee "$WORK/downloads/verify-pack.txt"
printf 'engine pack: OK arch=%s dmg=%s bytes=%s\n' "$ARCH" "$DMG" "$(stat -f%z "$DMG")"
