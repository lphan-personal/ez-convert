#!/usr/bin/env bash
# =============================================================================
# win-engine-pack.sh — assemble, strip, gate and package the Windows engine pack.
#
# The Windows installer (`docs/PACKAGING-WINDOWS.md`) bundles its engines inside
# the NSIS payload, which only works on a build host that can reach the engine
# sources. This script is the other half: it produces the engines as a standalone,
# versioned pack — `windows-engine-pack-<arch>.zip` — so a build machine with no
# outbound network can still ship them, and so the exact bytes can be attached to
# a Release (< 1 GB) and installed once by the user.
#
# It runs on a GitHub `windows-latest` runner under MSYS2 (bash + coreutils +
# binutils + python). Nothing is built on the engines' behalf except the parts
# we assemble: vips is the S2-dist proven GPL-free build, ffmpeg is the pinned
# LGPL autobuild, LibreOffice is the pinned TDF MSI tree.
#
# What it produces, in <out>:
#   windows-engine-pack-x86_64.zip        the payload
#   windows-engine-pack-x86_64.zip.sha256 its digest
# and inside the zip, the layout `EnginePaths::from_root` (src-tauri/src/engine/
# proc.rs) resolves on Windows — the same shape the macOS pack uses:
#   engines/vips/{vips.exe,vipsheader.exe,vipsthumbnail.exe,*.dll,licenses/}
#   engines/ffmpeg/{ffmpeg.exe,ffprobe.exe}
#   engines/soffice/program/{soffice.com,...} + share/ readmes/ presets/ ...
#   licenses/                              notices, digests, gate output
#   MANIFEST.sha256                        every file, hashed
#   README.txt                             what it is / how to install
#
# USAGE
#   scripts/win-engine-pack.sh --out <dir> [--arch x86_64]
#                              [--vips <tar.gz|dir>] [--ffmpeg <zip>] [--lo <msi>]
#                              [--skip-strip] [--work <dir>]
#   Env: EZC_WORK (scratch dir)
#
# EXIT: 0 packaged and verified, 1 any stage failed, 2 usage.
# =============================================================================
set -euo pipefail

ARCH="x86_64"
OUT=""
VIPS_SRC=""
FFMPEG_ZIP=""
LO_MSI=""
SKIP_STRIP=0
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# ---- pinned inputs -----------------------------------------------------------
# Every input is identified by a digest that is also recorded in
# docs/shipped-inventory.tsv (policy R6: a shipped file is never unnamed and
# never unhashed). A digest here and the inventory row must agree; the licence
# gate re-checks the delivered bytes against the inventory further down.
VIPS_TARBALL_SHA256="0dff47413690bf9ae3d0599e5fb4a04ff03419dcb191af80611fbcb73b2b9dc9"
VIPS_TARBALL="$REPO_ROOT/packaging/windows/inputs/vips-8.18.7-spec-aom-dist.tar.gz"
VIPS_FILES_EXPECTED=43

# BtbN FFmpeg-Builds, LGPL variant, the dated tag that carries build
# N-128273-g836436cfb9 (the build the inventory pins). Dated tags are immutable,
# so this URL is reproducible; `latest` is not and must never be used here.
FFMPEG_URL_DEFAULT="https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-11-01-12-49/ffmpeg-N-128273-g836436cfb9-win64-lgpl.zip"
FFMPEG_ZIP_SHA256="00bfdb3e67d674822e447d12bfa83db5910ed7618b0dc11bd4f6f53c2f798c61"
FFMPEG_EXE_SHA256="da90280ea6334d65ba9082a007251022ea67b226543cacd3a9f7aaab00338d52"
FFPROBE_EXE_SHA256="894f68a5f40b628bc2fe0641b9100ae93161797063855c74acbab1258d75bdc4"

LO_VERSION="${LO_VERSION:-26.8.2}"
LO_URL_DEFAULT="https://download.documentfoundation.org/libreoffice/stable/${LO_VERSION}/win/x86_64/LibreOffice_${LO_VERSION}_Win_x86-64.msi"
LO_MSI_SHA256="ff4b911403b67ee9e6249f820f9782c44bb32c94fdaa45b0da53a8e419e5307a"

# < 1 GB: a GitHub Release asset limit is 2 GB but an attached artifact the user
# downloads must stay reasonable; the acceptance criterion is < 1 GB.
MAX_PACK_BYTES=1073741824

while [ $# -gt 0 ]; do
  case "$1" in
    --out) OUT="${2:?--out needs a directory}"; shift 2 ;;
    --arch) ARCH="${2:?--arch needs x86_64}"; shift 2 ;;
    --vips) VIPS_SRC="${2:?}"; shift 2 ;;
    --ffmpeg) FFMPEG_ZIP="${2:?}"; shift 2 ;;
    --lo) LO_MSI="${2:?}"; shift 2 ;;
    --skip-strip) SKIP_STRIP=1; shift ;;
    --work) EZC_WORK="${2:?}"; shift 2 ;;
    -h|--help) sed -n '2,42p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

[ -n "$OUT" ] || { echo "usage: $0 --out <dir> [--arch x86_64]" >&2; exit 2; }
[ "$ARCH" = "x86_64" ] || { echo "unsupported --arch $ARCH (only x86_64 is built)" >&2; exit 2; }

case "$(uname -s)" in
  MSYS_NT*|MINGW64_NT*|MINGW32_NT*|CYGWIN_NT*) : ;;
  *) echo "this script only runs on Windows (MSYS2/Cygwin): it needs msiexec, PE binaries and a real Windows" >&2; exit 2 ;;
esac

for tool in curl sha256sum find awk objdump python3 cygpath unzip zip msiexec; do
  command -v "$tool" >/dev/null || { echo "missing tool: $tool" >&2; exit 1; }
done

WORK="${EZC_WORK:-$PWD/work}"
PACK="$WORK/pack"
DL="$WORK/downloads"
mkdir -p "$PACK/engines" "$DL" "$WORK/smoke" "$OUT"

step() { printf '\n=== %s\n' "$*"; }
note() { printf '    %s\n' "$*"; }
die()  { printf 'FAIL: %s\n' "$*" >&2; exit 1; }

# POSIX path -> Windows path, for the tools that only speak Windows (msiexec,
# soffice's --outdir, the file:// URL of a LibreOffice profile).
win() { cygpath -w "$1"; }

# sha256 of a file, lower case, no filename.
digest() { sha256sum "$1" | cut -d' ' -f1; }

# ---------------------------------------------------------------- 1. fetch
step "1/9 inputs (pinned digests)"
VIPS_SRC="${VIPS_SRC:-$VIPS_TARBALL}"
[ -e "$VIPS_SRC" ] || die "vips dist not found: $VIPS_SRC"
if [ -d "$VIPS_SRC" ]; then
  note "vips: directory $VIPS_SRC (not hash-pinned; the licence gate still hashes every file)"
  mkdir -p "$WORK/vips-src"
  cp -a "$VIPS_SRC"/. "$WORK/vips-src"/
  VIPS_TREE="$WORK/vips-src"
else
  got="$(digest "$VIPS_SRC")"
  [ "$got" = "$VIPS_TARBALL_SHA256" ] || die "vips dist digest drift: $got != $VIPS_TARBALL_SHA256"
  note "vips: $(basename "$VIPS_SRC") sha256 $got"
  VIPS_TREE="$WORK/vips-src"
  mkdir -p "$VIPS_TREE"
  tar xzf "$VIPS_SRC" -C "$VIPS_TREE"
fi

if [ -n "$FFMPEG_ZIP" ] && [ -f "$FFMPEG_ZIP" ]; then
  note "ffmpeg: $FFMPEG_ZIP (local; digest checked below)"
else
  FFMPEG_ZIP="$DL/ffmpeg-win64-lgpl.zip"
  if [ ! -f "$FFMPEG_ZIP" ]; then
    note "download $FFMPEG_URL_DEFAULT"
    curl -fSL --retry 3 --retry-delay 5 -o "$FFMPEG_ZIP" "$FFMPEG_URL_DEFAULT"
  fi
fi
got="$(digest "$FFMPEG_ZIP")"
[ "$got" = "$FFMPEG_ZIP_SHA256" ] || die "ffmpeg zip digest drift: $got != $FFMPEG_ZIP_SHA256"
note "ffmpeg: $(basename "$FFMPEG_ZIP") sha256 $got"

if [ -n "$LO_MSI" ] && [ -f "$LO_MSI" ]; then
  note "LibreOffice: $LO_MSI (local; digest checked below)"
else
  LO_MSI="$DL/LibreOffice_${LO_VERSION}_Win_x86-64.msi"
  if [ ! -f "$LO_MSI" ]; then
    note "download $LO_URL_DEFAULT"
    curl -fSL --retry 3 --retry-delay 10 -o "$LO_MSI" "$LO_URL_DEFAULT"
  fi
fi
got="$(digest "$LO_MSI")"
[ "$got" = "$LO_MSI_SHA256" ] || die "LibreOffice MSI digest drift: $got != $LO_MSI_SHA256"
note "LibreOffice: $(basename "$LO_MSI") $(stat -c%s "$LO_MSI" 2>/dev/null || wc -c < "$LO_MSI") B sha256 $got"

# ----------------------------------------------------------------- 2. vips
step "2/9 libvips 8.18.7 spec-aom ($ARCH) — the S2-dist GPL-free build"
VIPS_OUT="$PACK/engines/vips"
mkdir -p "$VIPS_OUT"
cp -a "$VIPS_TREE"/. "$VIPS_OUT"/
n=$(find "$VIPS_OUT" -type f | wc -l | tr -d ' ')
[ "$n" -eq "$VIPS_FILES_EXPECTED" ] || {
  echo "vips dist has $n files, expected $VIPS_FILES_EXPECTED:" >&2
  find "$VIPS_OUT" -type f | sed 's/^/    /' >&2
  exit 1; }
[ -f "$VIPS_OUT/vips.exe" ] || die "vips dist has no vips.exe"
# The historical GPL contaminant. Its absence is a licence property, not a
# preference: jbigkit is GPL-2.0 and rode in through libtiff until libtiff was
# rebuilt with -Djbig=OFF (docs/LICENSES.md §4).
if find "$VIPS_OUT" -iname 'libjbig*' | grep -q .; then
  die "vips dist contains libjbig (GPL-2.0 contaminant)"
fi
# Relocated-tree run: vips.exe must find its DLLs beside itself.
"$VIPS_OUT/vips.exe" --version >/dev/null || die "vips.exe does not run from the pack"
note "$n files, $(du -sb "$VIPS_OUT" | cut -f1) B, $("$VIPS_OUT/vips.exe" --version 2>&1 | head -1)"
grep -qiE 'image quantisation .*: false' "$VIPS_OUT/vips-config.txt" \
  || die "vips-config.txt does not show image quantisation OFF (GPL imagequant path)"
grep -qiE 'HEIC/AVIF load/save with libheif: true' "$VIPS_OUT/vips-config.txt" \
  || die "vips-config.txt does not show HEIC/AVIF with libheif ON"
note "config: imagequant=false, poppler/pdfium off, libheif=true (asserted from vips-config.txt)"

# --------------------------------------------------------------- 3. ffmpeg
step "3/9 ffmpeg + ffprobe (BtbN LGPL autobuild, no GPL)"
FFMPEG_OUT="$PACK/engines/ffmpeg"
mkdir -p "$FFMPEG_OUT"
mkdir -p "$WORK/ffmpeg-x"
unzip -q -o "$FFMPEG_ZIP" -d "$WORK/ffmpeg-x"
ffbin="$(find "$WORK/ffmpeg-x" -type f -name ffmpeg.exe -path '*/bin/*' | head -1)"
[ -n "$ffbin" ] || { find "$WORK/ffmpeg-x" -maxdepth 4 -type d >&2; die "no ffmpeg.exe in the zip"; }
ffdir="$(dirname "$ffbin")"
cp -f "$ffbin" "$ffdir/ffprobe.exe" "$FFMPEG_OUT/"
[ "$(digest "$FFMPEG_OUT/ffmpeg.exe")" = "$FFMPEG_EXE_SHA256" ] || die "ffmpeg.exe digest drift"
[ "$(digest "$FFMPEG_OUT/ffprobe.exe")" = "$FFPROBE_EXE_SHA256" ] || die "ffprobe.exe digest drift"
# The build's own words, not ours: --enable-gpl/--enable-nonfree must not appear.
"$FFMPEG_OUT/ffmpeg.exe" -hide_banner -buildconf 2>&1 | tee "$WORK/ffmpeg-buildconf.txt" >/dev/null
if grep -qE -- '--enable-gpl|--enable-nonfree' "$WORK/ffmpeg-buildconf.txt"; then
  grep -nE -- '--enable-gpl|--enable-nonfree' "$WORK/ffmpeg-buildconf.txt" >&2
  die "ffmpeg build advertises --enable-gpl/--enable-nonfree"
fi
"$FFMPEG_OUT/ffprobe.exe" -hide_banner -version >/dev/null || die "ffprobe.exe does not run"
note "ffmpeg $("$FFMPEG_OUT/ffmpeg.exe" -hide_banner -version 2>&1 | head -1) (enable-gpl=0)"
note "ffprobe $("$FFMPEG_OUT/ffprobe.exe" -hide_banner -version 2>&1 | head -1)"

# ---------------------------------------------------------- 4. LibreOffice
step "4/9 LibreOffice $LO_VERSION ($ARCH) — administrative extraction of the MSI"
LO_OUT="$PACK/engines/soffice"
mkdir -p "$LO_OUT"
msilog="$WORK/msiexec.log"
rc=0
# /a = administrative install: the MSI's file tree is laid out under TARGETDIR
# without registering anything on the machine — the Windows equivalent of
# unpacking the macOS .app or the Linux deb tree.
MSYS2_ARG_CONV_EXCL='*' msiexec /a "$(win "$LO_MSI")" /qn /norestart \
  TARGETDIR="$(win "$LO_OUT")" /l*v "$(win "$msilog")" || rc=$?
note "msiexec /a rc=$rc"
if [ "$rc" -ne 0 ] && [ "$rc" -ne 3010 ]; then
  tail -40 "$msilog" >&2 || true
  die "msiexec administrative install failed (rc=$rc)"
fi
# Where the tree actually landed: TARGETDIR, or one directory below it on some
# MSI versions. Detect from the launcher rather than assuming.
launcher="$(find "$LO_OUT" -maxdepth 4 -type f -name soffice.com | head -1)"
[ -n "$launcher" ] || {
  find "$LO_OUT" -maxdepth 3 -type d >&2
  die "no program/soffice.com after extraction"
}
LO_ROOT="$(cd "$(dirname "$launcher")/.." && pwd)"
if [ "$LO_ROOT" != "$LO_OUT" ]; then
  note "tree landed in ${LO_ROOT#$WORK/}; moving it to the pack root"
  shopt -s dotglob
  mv "$LO_ROOT"/* "$LO_OUT"/
  shopt -u dotglob
  rmdir "$LO_ROOT" 2>/dev/null || true
fi
[ -f "$LO_OUT/program/soffice.com" ] || die "program/soffice.com is not at the pack root"
[ -d "$LO_OUT/share" ] || die "share/ is not at the pack root — not a LibreOffice tree"
note "$(find "$LO_OUT" -type f | wc -l | tr -d ' ') files, $(du -sb "$LO_OUT" | cut -f1) B before the strip"

# Top-level inventory: an administrative install lays out *everything* the MSI
# can place, which is not the same set as an installed program directory. Print
# it, because the next decision (`Fonts/`) is made from these numbers.
for d in "$LO_OUT"/*/; do
  [ -d "$d" ] || continue
  printf '    %-14s %6s files %12s B\n' "$(basename "$d")" \
    "$(find "$d" -type f | wc -l | tr -d ' ')" "$(du -sb "$d" | cut -f1)"
done

# `Fonts/` is the MSI's copy of the fonts the Windows installer drops into
# C:\Windows\Fonts — not something LibreOffice loads from a program directory
# (it reads fonts from the OS, and its own `share/fonts/truetype` for the
# document-embedded set). Two reasons it does not belong in the pack:
#   * it duplicates ~5,500 files of payload the engine never opens, and
#   * it carries the five GPL-2.0 Culmus families, which the licence decision
#     (t_20fae747) strips everywhere else — shipping them here would put GPL
#     font software back into a PDF that embeds them.
# An installed app has those fonts from the OS, so dropping the tree keeps the
# pack equivalent to the installed layout.
if [ -d "$LO_OUT/Fonts" ]; then
  note "dropping Fonts/ ($(find "$LO_OUT/Fonts" -type f | wc -l | tr -d ' ') files, $(du -sb "$LO_OUT/Fonts" | cut -f1) B)"
  rm -rf "$LO_OUT/Fonts"
  if [ -d "$LO_OUT/Fonts" ]; then die "could not remove the MSI font payload"; fi
fi

# ------------------------------------------------------------------ 5. strip
step "5/9 strip the content the licence decision removed (t_20fae747)"
if [ "$SKIP_STRIP" = "1" ]; then
  note "SKIPPED by request — the licence gate below will reject the trees"
else
  python3 "$REPO_ROOT/scripts/strip-engine-pack.py" --pack "$LO_OUT" \
    --manifest "$WORK/strip-manifest.tsv" | tee "$WORK/strip.log"
fi
# A strip that silently did nothing must not pass as a licence decision: assert
# the removals here as well, because the inventory rows for gallery/help are
# written against the *pack-relative* paths and a survivor is only caught if the
# path shape matches.
for gone in "$LO_OUT/share/gallery" "$LO_OUT/gallery" "$LO_OUT/help"; do
  if [ -d "$gone" ] && [ -n "$(find "$gone" -type f -print -quit 2>/dev/null)" ]; then
    die "strip incomplete: files still under $gone"
  fi
done
survivor="$(find "$LO_OUT" \( -name 'DavidCLM-*' -o -name 'FrankRuehlCLM-*' \
  -o -name 'MiriamCLM-*' -o -name 'MiriamMonoCLM-*' -o -name 'NachlieliCLM-*' \) -print -quit)"
[ -z "$survivor" ] || die "strip incomplete: GPL-2.0 Culmus font still shipped: $survivor"
note "strip verified: gallery, help and the five Culmus families are gone"
note "$(find "$LO_OUT" -type f | wc -l | tr -d ' ') files, $(du -sb "$LO_OUT" | cut -f1) B after the strip"

# -------------------------------------------------- 6. dependency closure
step "6/9 dependency closure: every imported DLL resolves inside the pack or is a Windows system DLL"
# The Windows counterpart of the macOS otool gate. A PE import table names a DLL
# by bare name; the loader searches the executable's own directory first (that
# is what makes this layout work), then the system32 directory. So every import
# must be either a file in the same pack directory or a library Windows itself
# ships. Anything else is a file the user's machine may not have — the failure
# mode is `The code execution cannot proceed because <x>.dll was not found`.
SYSTEM_DLL_RE='^(api-ms-win-.*|ext-ms-win-.*|kernel32|kernelbase|user32|gdi32|advapi32|shell32|shlwapi|ole32|oleaut32|comctl32|comdlg32|ws2_32|wsock32|psapi|version|bcrypt|ncrypt|ntdll|rpcrt4|crypt32|secur32|wintrust|imm32|winmm|msimg32|opengl32|glu32|dwmapi|dwrite|d3d11|dxgi|dxva2|uxtheme|dnsapi|iphlpapi|mpr|netapi32|userenv|wldap32|wevtapi|mswsock|msvcrt|ucrtbase|vcruntime140|vcruntime140_1|msvcp140.*|concrt140|mfc140.*|oleacc|setupapi|cfgmgr32|winspool.drv|hid|powrprof|dbghelp|dbgeng|imagehlp|winhttp|wininet|urlmon|normaliz|sechost|profapi|cryptbase|msasn1|gdiplus|wtsapi32)$'

closure_fail=0
closure_assumed=""
closure_checked=0
closure_check_tree() { # <dir> — every .exe/.dll directly in <dir> must resolve
  local dir="$1" f dll base missing
  while IFS= read -r f; do
    closure_checked=$((closure_checked + 1))
    missing=""
    while IFS= read -r dll; do
      [ -n "$dll" ] || continue
      base="$(printf '%s' "$dll" | tr '[:upper:]' '[:lower:]')"
      if printf '%s' "$base" | grep -qE "$SYSTEM_DLL_RE"; then
        closure_assumed="${closure_assumed}${base} "
        continue
      fi
      if [ -f "$dir/$dll" ] || [ -f "$dir/$base" ]; then continue; fi
      # an import that resolves to a real file with different case on disk
      if [ -n "$(find "$dir" -maxdepth 1 -iname "$dll" -print -quit)" ]; then continue; fi
      missing="${missing}${dll} "
    done < <(objdump -p "$f" 2>/dev/null | awk '/DLL Name:/{print $3}')
    if [ -n "$missing" ]; then
      printf '    UNRESOLVED %s -> %s\n' "${f#"$PACK"/}" "$missing" >&2
      closure_fail=$((closure_fail + 1))
    fi
  done < <(find "$dir" -maxdepth 1 -type f \( -name '*.exe' -o -name '*.dll' \) | sort)
}
closure_check_tree "$VIPS_OUT"
closure_check_tree "$FFMPEG_OUT"
# LibreOffice is an 8,000-file self-contained tree with its own MSVC runtime; the
# meaningful check there is that its *loader-facing* objects resolve, and the
# real proof is the document conversion in step 8 (a missing DLL shows up as a
# conversion that produces no file, which that step refuses).
if [ "$closure_fail" -ne 0 ]; then
  die "dependency closure incomplete: $closure_fail object(s) import a DLL that is neither in the pack nor a Windows system DLL"
fi
note "$closure_checked PE objects checked, 0 unresolved imports"
note "system DLLs assumed (Windows-provided): $(printf '%s' "$closure_assumed" | tr ' ' '\n' | sort -u | tr '\n' ' ')"

# The banned-import view, kept separate from the closure above: these are the
# GPL components the S2 work removed, and a silent return would be a licence
# defect even if the closure still resolved.
step "    banned-import audit (imagequant / poppler / magick / x265 / x264 / jbig)"
banned=0
while IFS= read -r f; do
  hit="$(objdump -p "$f" 2>/dev/null | grep -iE 'DLL Name:.*(imagequant|poppler|magick|x265|x264|jbig|poppler)' || true)"
  [ -z "$hit" ] || { echo "    BANNED IMPORT in ${f#"$PACK"/}:" >&2; echo "$hit" >&2; banned=$((banned + 1)); }
done < <(find "$VIPS_OUT" "$FFMPEG_OUT" -maxdepth 1 -type f \( -name '*.exe' -o -name '*.dll' \) | sort)
if find "$VIPS_OUT" "$FFMPEG_OUT" -iname '*imagequant*' -o -iname '*poppler*' | grep -q .; then
  echo "    BANNED FILE present" >&2; banned=$((banned + 1))
fi
[ "$banned" -eq 0 ] || die "$banned banned-import hit(s) in the assembled engines"
note "PASS: no imagequant / poppler / magick / x265 / x264 / jbig import in the pack"

# ------------------------------------------------------------ 7. licence gate
step "7/9 licence gate over docs/shipped-inventory.tsv (fail-closed)"
bash "$REPO_ROOT/scripts/dist-license-gate.sh" --check-inventory | tee "$WORK/licence-inventory.txt" | tail -2
fails=0
# Each engine tree is scanned from its own root: the inventory names files
# relative to the artifact root (`program/**`, `share/*`, `libvips-42.dll`), which
# is exactly the root each engine ships at inside the pack.
for tree in "$VIPS_OUT" "$FFMPEG_OUT" "$LO_OUT"; do
  printf '    scan %s\n' "${tree#"$PACK"/}"
  if ! bash "$REPO_ROOT/scripts/dist-license-gate.sh" --scan "$tree" >>"$WORK/licence-scan.txt" 2>&1; then
    fails=$((fails + 1))
  fi
  tail -1 "$WORK/licence-scan.txt" | sed 's/^/    /'
done
[ "$fails" -eq 0 ] || { grep -E '^FAIL' "$WORK/licence-scan.txt" | head -40 >&2; die "$fails engine tree(s) failed the licence gate"; }
note "each engine tree scanned from its own root (the inventory names files relative to it)"

# ---------------------------------------------------------------- 8. smoke
step "8/9 smoke: a real conversion per family — the output is judged, never the exit code"
FIX="$REPO_ROOT/fixtures"
SMOKE="$WORK/smoke"
image_ok=0 audio_ok=0 video_ok=0 doc_ok=0

# image: png -> jpg, then read the dimensions back out of the product and compare
# them with the source's own — a zero-byte or truncated JPEG would fail here.
"$VIPS_OUT/vips.exe" copy "$(win "$FIX/sample.png")" "$(win "$SMOKE/image.jpg")[Q=85]" \
  >"$SMOKE/image.log" 2>&1 || { cat "$SMOKE/image.log" >&2; die "vips png->jpg failed"; }
[ -s "$SMOKE/image.jpg" ] || { cat "$SMOKE/image.log" >&2; die "vips produced no image output"; }
sw="$("$VIPS_OUT/vipsheader.exe" -f width "$(win "$FIX/sample.png")" 2>&1 | tr -d '\r\n')"
sh="$("$VIPS_OUT/vipsheader.exe" -f height "$(win "$FIX/sample.png")" 2>&1 | tr -d '\r\n')"
ow="$("$VIPS_OUT/vipsheader.exe" -f width "$(win "$SMOKE/image.jpg")" 2>&1 | tr -d '\r\n')"
oh="$("$VIPS_OUT/vipsheader.exe" -f height "$(win "$SMOKE/image.jpg")" 2>&1 | tr -d '\r\n')"
[ "$sw" = "$ow" ] && [ "$sh" = "$oh" ] || die "image is ${ow}x${oh}, source is ${sw}x${sh}"
image_ok=1
note "image   png -> jpg $(wc -c <"$SMOKE/image.jpg") B, ${ow}x${oh} (source ${sw}x${sh})"

# image (second path): the HEIC decode that the stock builds cannot do.
"$VIPS_OUT/vips.exe" copy "$(win "$FIX/sample.heic")" "$(win "$SMOKE/heic.jpg")" \
  >"$SMOKE/heic.log" 2>&1 || { cat "$SMOKE/heic.log" >&2; die "vips heic->jpg failed (libheif decode)"; }
[ -s "$SMOKE/heic.jpg" ] || die "vips produced no output for the HEIC fixture"
note "image   heic -> jpg $(wc -c <"$SMOKE/heic.jpg") B (libheif decode)"

# audio: wav -> mp3 through libmp3lame, then ffprobe the product's codec.
"$FFMPEG_OUT/ffmpeg.exe" -nostdin -v error -y "$(win "$FIX/sample.wav")" \
  -codec:a libmp3lame -q:a 2 "$(win "$SMOKE/audio.mp3")" >"$SMOKE/audio.log" 2>&1 \
  || { cat "$SMOKE/audio.log" >&2; die "ffmpeg wav->mp3 failed"; }
[ -s "$SMOKE/audio.mp3" ] || die "ffmpeg produced no audio output"
codec="$("$FFMPEG_OUT/ffprobe.exe" -v error -select_streams a:0 -show_entries stream=codec_name -of default=nw=1:nk=1 "$(win "$SMOKE/audio.mp3")" 2>&1 | head -1)"
[ "$codec" = "mp3" ] || die "audio product codec is '$codec', expected mp3"
audio_ok=1
note "audio   wav -> mp3 $(wc -c <"$SMOKE/audio.mp3") B, codec=$codec (libmp3lame)"

# video: mp4 -> webm exactly as the app's own route builds it
# (`VideoEncode::for_target(Webm)`: libvpx-vp9 + libopus, src-tauri/src/engine/ffmpeg.rs).
"$FFMPEG_OUT/ffmpeg.exe" -nostdin -v error -y -i "$(win "$FIX/sample.mp4")" \
  -c:v libvpx-vp9 -b:v 200k -c:a libopus -f webm "$(win "$SMOKE/video.webm")" \
  >"$SMOKE/video.log" 2>&1 || { cat "$SMOKE/video.log" >&2; die "ffmpeg mp4->webm failed"; }
[ -s "$SMOKE/video.webm" ] || die "ffmpeg produced no video output"
vcodec="$("$FFMPEG_OUT/ffprobe.exe" -v error -select_streams v:0 -show_entries stream=codec_name -of default=nw=1:nk=1 "$(win "$SMOKE/video.webm")" 2>&1 | head -1)"
[ "$vcodec" = "vp9" ] || die "video product codec is '$vcodec', expected vp9"
video_ok=1
note "video   mp4 -> webm $(wc -c <"$SMOKE/video.webm") B, codec=$vcodec (libvpx-vp9 + libopus)"

# document: docx -> pdf through the console launcher, in a throw-away profile.
# LibreOffice is the engine that lies about success, so the product is what is
# judged: a PDF that opens, has a page object and carries the document's text.
LO_PROFILE="file:///$(cygpath -m "$SMOKE/lo-profile")"
"$LO_OUT/program/soffice.com" --headless --norestore --nologo --nodefault \
  -env:UserInstallation="$LO_PROFILE" \
  --convert-to pdf --outdir "$(win "$SMOKE")" "$(win "$FIX/sample.docx")" \
  >"$SMOKE/doc.log" 2>&1 || true   # its exit code is not evidence; the product is
if [ ! -s "$SMOKE/sample.pdf" ]; then
  echo "--- LibreOffice's own words ---" >&2
  tail -30 "$SMOKE/doc.log" >&2
  die "LibreOffice produced no PDF for fixtures/sample.docx"
fi
head -c 5 "$SMOKE/sample.pdf" | grep -q '%PDF-' || die "the document product is not a PDF"
tail -c 32 "$SMOKE/sample.pdf" | grep -q '%%EOF' || die "the document product is a truncated PDF"
# A page object in the clear (`/Type /Page`, not `/Pages`); LibreOffice writes
# its page tree uncompressed. A PDF with no page object is a header-only stub.
pages="$(grep -ao '/Type[[:space:]]*/Page[^s]' "$SMOKE/sample.pdf" | wc -l | tr -d ' ')"
[ "${pages:-0}" -ge 1 ] || die "the document product has no page object"

# The text itself, through the engine's plain-text writer: MARKER-ALPHA is in
# fixtures/sample.docx and only a real conversion carries it across.
"$LO_OUT/program/soffice.com" --headless --norestore --nologo --nodefault \
  -env:UserInstallation="$LO_PROFILE" \
  --convert-to 'txt:Text (encoded):UTF8' --outdir "$(win "$SMOKE")" "$(win "$FIX/sample.docx")" \
  >"$SMOKE/doc-txt.log" 2>&1 || true
if [ ! -s "$SMOKE/sample.txt" ]; then
  echo "--- LibreOffice's own words (txt) ---" >&2
  tail -30 "$SMOKE/doc-txt.log" >&2
  die "LibreOffice produced no text for fixtures/sample.docx"
fi
grep -q 'MARKER-ALPHA' "$SMOKE/sample.txt" \
  || { head -20 "$SMOKE/sample.txt" >&2; die "the document product lost MARKER-ALPHA — the text did not convert"; }
grep -q 'café' "$SMOKE/sample.txt" || die "the document product lost the UTF-8 characters (café)"
doc_ok=1
note "document docx -> pdf $(wc -c <"$SMOKE/sample.pdf") B, $pages page object(s); docx -> txt $(wc -c <"$SMOKE/sample.txt") B with MARKER-ALPHA + café (LibreOffice $LO_VERSION)"

[ "$image_ok$audio_ok$video_ok$doc_ok" = "1111" ] || die "a family did not convert"
note "FAMILY image PASS / audio PASS / video PASS / document PASS"

# The layout contract: the names `EnginePaths::from_root` (src-tauri/src/engine/
# proc.rs) resolves on Windows. If any of these moves, the app finds no engine
# and every conversion reports MissingEngine — so assert them, not the directory.
step "    layout contract (what EnginePaths::from_root resolves on Windows)"
for rel in engines/vips/vips.exe engines/vips/vipsheader.exe \
           engines/ffmpeg/ffmpeg.exe engines/ffmpeg/ffprobe.exe \
           engines/soffice/program/soffice.com; do
  [ -f "$PACK/$rel" ] || die "missing $rel — adopt_pack()/from_root() would not find this engine"
  note "ok $rel"
done

# -------------------------------------------------------------- 9. package
step "9/9 package windows-engine-pack-$ARCH.zip"
LIC="$PACK/licenses"
mkdir -p "$LIC"
cp "$REPO_ROOT/docs/licenses/THIRD-PARTY-NOTICES.md" "$LIC/"
cp "$WORK/ffmpeg-buildconf.txt" "$LIC/ffmpeg-buildconf.txt"
cp "$WORK/msiexec.log" "$LIC/libreoffice-msi-admin-install.log"
printf '%s  %s\n' "$LO_MSI_SHA256" "LibreOffice_${LO_VERSION}_Win_x86-64.msi" > "$LIC/libreoffice-msi.sha256"
cp "$WORK/strip-manifest.tsv" "$LIC/strip-manifest.tsv" 2>/dev/null || true
cp "$WORK/licence-inventory.txt" "$LIC/licence-inventory.txt" 2>/dev/null || true
cp "$WORK/licence-scan.txt" "$LIC/licence-scan.txt" 2>/dev/null || true
cp "$REPO_ROOT/packaging/windows/README-pack.txt" "$PACK/README.txt"
# The gate's notice row for `licenses/**` is status notice-text. Attribution for
# the pack's own root files is checked on a projection of exactly those names
# (hard links, no copy) — the same technique stage-engines.sh uses for the Tauri
# sidecar names, because the inventory rows are written against the *final*
# names, not against the staging paths.
ROOTPROJ="$WORK/rootproj"
mkdir -p "$ROOTPROJ/licenses"
ln -f "$PACK/README.txt" "$ROOTPROJ/README.txt"
ln -f "$PACK/licenses/THIRD-PARTY-NOTICES.md" "$ROOTPROJ/licenses/THIRD-PARTY-NOTICES.md"
ln -f "$PACK/licenses/ffmpeg-buildconf.txt" "$ROOTPROJ/licenses/ffmpeg-buildconf.txt"
bash "$REPO_ROOT/scripts/dist-license-gate.sh" --scan "$ROOTPROJ" >>"$WORK/licence-scan.txt" 2>&1 \
  || { grep -E '^FAIL' "$WORK/licence-scan.txt" | tail -10 >&2; die "the pack's root files do not attribute to the inventory"; }
tail -1 "$WORK/licence-scan.txt" | sed 's/^/    /'
# The notices we ship must be the committed ones, not a stale copy of them.
[ "$(digest "$LIC/THIRD-PARTY-NOTICES.md")" = "$(digest "$REPO_ROOT/docs/licenses/THIRD-PARTY-NOTICES.md")" ] \
  || die "licenses/THIRD-PARTY-NOTICES.md is not the committed notice"
note "notices: licenses/THIRD-PARTY-NOTICES.md is byte-identical to docs/licenses/"

( cd "$PACK" && find . -type f ! -name MANIFEST.sha256 -print0 \
  | xargs -0 sha256sum > MANIFEST.sha256 )
note "manifest: $(wc -l < "$PACK/MANIFEST.sha256" | tr -d ' ') files"

ZIP="$OUT/windows-engine-pack-$ARCH.zip"
rm -f "$ZIP"
( cd "$PACK" && zip -r -9 -q "$(win "$ZIP")" . -x '*/.*' )
[ -s "$ZIP" ] || die "the zip was not produced"
# Verify the artifact as a user receives it: unzip elsewhere and check every hash.
VERIFY="$WORK/verify"
mkdir -p "$VERIFY"
unzip -q -o "$ZIP" -d "$VERIFY"
( cd "$VERIFY" && sha256sum -c --quiet MANIFEST.sha256 ) || die "MANIFEST.sha256 does not verify after unzip"
[ -f "$VERIFY/engines/soffice/program/soffice.com" ] || die "the zip loses the LibreOffice launcher"
[ -f "$VERIFY/engines/vips/vips.exe" ] || die "the zip loses vips.exe"
note "artifact verified: unpacked and every MANIFEST digest re-checked"

bytes="$(wc -c < "$ZIP" | tr -d ' ')"
[ "$bytes" -lt "$MAX_PACK_BYTES" ] || die "pack is $bytes B, over the ${MAX_PACK_BYTES} B (<1 GiB) limit"
digest "$ZIP" > "$OUT/windows-engine-pack-$ARCH.zip.sha256.tmp"
mv "$OUT/windows-engine-pack-$ARCH.zip.sha256.tmp" "$OUT/windows-engine-pack-$ARCH.zip.sha256"
note "$(du -h "$ZIP" | cut -f1) → $ZIP"
note "sha256 $(cat "$OUT/windows-engine-pack-$ARCH.zip.sha256")"

cat > "$OUT/build-report.txt" <<EOF
engine pack: OK
arch: $ARCH
zip: $(basename "$ZIP")
bytes: $bytes
sha256: $(cat "$OUT/windows-engine-pack-$ARCH.zip.sha256")
files in manifest: $(wc -l < "$PACK/MANIFEST.sha256" | tr -d ' ')
vips files: $(find "$VIPS_OUT" -type f | wc -l | tr -d ' ')
soffice files after strip: $(find "$LO_OUT" -type f | wc -l | tr -d ' ')
inputs:
  vips dist    sha256=$VIPS_TARBALL_SHA256
  ffmpeg zip   sha256=$FFMPEG_ZIP_SHA256 ($(basename "$FFMPEG_ZIP"))
  ffmpeg.exe   sha256=$FFMPEG_EXE_SHA256
  ffprobe.exe  sha256=$FFPROBE_EXE_SHA256
  LibreOffice  sha256=$LO_MSI_SHA256 (LibreOffice_${LO_VERSION}_Win_x86-64.msi)
licence gate: 0 failures over $(find "$VIPS_OUT" "$FFMPEG_OUT" "$LO_OUT" -type f | wc -l | tr -d ' ') shipped files
smoke: image PASS (png->jpg, heic->jpg) audio PASS (wav->mp3) video PASS (mp4->webm) document PASS (docx->pdf)
EOF
cat "$OUT/build-report.txt"
printf 'engine pack: OK arch=%s zip=%s bytes=%s\n' "$ARCH" "$ZIP" "$bytes"
