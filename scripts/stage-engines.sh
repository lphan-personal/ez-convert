#!/usr/bin/env bash
# =============================================================================
# stage-engines.sh — assemble the Windows bundle inputs for P2.
#
# Produces, under --out (default: src-tauri):
#   binaries/ffmpeg-x86_64-pc-windows-msvc.exe      externalBin (Tauri signs it)
#   binaries/ffprobe-x86_64-pc-windows-msvc.exe     externalBin
#   resources/engines/vips/                         vips.exe + DLL/module closure
#   resources/engines/soffice/                      LibreOffice tree, STRIPPED
#   resources/licenses/                             notices + per-engine licence files
#
# and refuses to finish unless scripts/dist-license-gate.sh --scan passes on the
# exact trees it produced (fail-closed: an unattributed shipped file fails).
#
# USAGE
#   scripts/stage-engines.sh --vips <dir|tar.gz> --ffmpeg <dir> [--lo-pack <dir>]
#                            [--pdfium <exe>] [--out src-tauri] [--dry-run]
#
# EXIT: 0 staged and licence-clean, 1 verification/gate failure, 2 usage.
# =============================================================================
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
out="$repo_root/src-tauri"
vips_src="" ffmpeg_src="" lo_src="" pdfium_src="" dry=0
target_triple="x86_64-pc-windows-msvc"
manifest="$repo_root/evidence/p2-stage-manifest.tsv"

log() { printf '%s\n' "$*"; }
die() { printf 'stage-engines: %s\n' "$*" >&2; exit 1; }

while [ $# -gt 0 ]; do
  case "$1" in
    --vips) vips_src="${2:?}"; shift 2 ;;
    --ffmpeg) ffmpeg_src="${2:?}"; shift 2 ;;
    --lo-pack) lo_src="${2:?}"; shift 2 ;;
    --pdfium) pdfium_src="${2:?}"; shift 2 ;;
    --out) out="${2:?}"; shift 2 ;;
    --dry-run) dry=1; shift ;;
    -h|--help) sed -n '2,20p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) die "unknown argument: $1 (usage: --vips --ffmpeg [--lo-pack] [--pdfium] [--out] [--dry-run])" ;;
  esac
done

[ -n "$vips_src" ] || die "--vips is required (the dist ships inside the installer)"
[ -n "$ffmpeg_src" ] || die "--ffmpeg is required (audio/video family)"

bin_dir="$out/binaries"
eng_dir="$out/resources/engines"
lic_dir="$out/resources/licenses"

run() { if [ "$dry" -eq 1 ]; then log "DRY-RUN would: $*"; else "$@"; fi; }

# --- 1. vips -----------------------------------------------------------------
vips_expect=43
vips_dst="$eng_dir/vips"
if [ "$dry" -eq 0 ]; then mkdir -p "$vips_dst"; rm -rf "${vips_dst:?}"/*; fi
if [ -d "$vips_src" ]; then
  run cp -a "$vips_src"/. "$vips_dst"/
else
  case "$vips_src" in
    *.tar.gz) run tar -xzf "$vips_src" -C "$vips_dst" ;;
    *.zip) run unzip -q -o "$vips_src" -d "$vips_dst" ;;
    *) die "vips source must be a directory, .tar.gz or .zip: $vips_src" ;;
  esac
fi
if [ "$dry" -eq 0 ]; then
  files=$(find "$vips_dst" -type f | wc -l)
  [ "$files" -eq "$vips_expect" ] || die "vips dist has $files files, expected $vips_expect"
  ! find "$vips_dst" -iname 'libjbig*' | grep -q . || die "vips dist contains libjbig (GPL-2.0 contaminant)"
  [ -f "$vips_dst/vips.exe" ] || die "vips dist has no vips.exe"
  log "vips   $files files, $(du -sb "$vips_dst" | cut -f1) B"
fi

# --- 2. ffmpeg / ffprobe (externalBin) --------------------------------------
for exe in ffmpeg ffprobe; do
  src="$ffmpeg_src/$exe.exe"
  [ -f "$src" ] || die "missing $src"
  if [ "$dry" -eq 0 ]; then
    mkdir -p "$bin_dir"
    cp -f "$src" "$bin_dir/$exe-$target_triple.exe"
    grep_count=$("$src" -hide_banner -buildconf 2>&1 | grep -c -- '--enable-gpl' || true)
    [ "$grep_count" -eq 0 ] || die "$exe.exe is a GPL build (--enable-gpl count $grep_count); shipping it breaks policy R6"
    log "$exe   $(stat -c%s "$bin_dir/$exe-$target_triple.exe") B (enable-gpl=0)"
  else
    log "DRY-RUN would: copy $src -> $bin_dir/$exe-$target_triple.exe"
  fi
done

# --- 3. LibreOffice pack (documents) ----------------------------------------
lo_dst=""
if [ -n "$lo_src" ]; then
  lo_dst="$eng_dir/soffice"
  if [ "$dry" -eq 0 ]; then
    rm -rf "$lo_dst"; mkdir -p "$lo_dst"
    cp -a "$lo_src"/. "$lo_dst"/
    python3 "$repo_root/scripts/strip-engine-pack.py" \
      --pack "$lo_dst" --manifest "$repo_root/evidence/engine-pack-strip-manifest.tsv"
    log "soffice stripped pack: $(find "$lo_dst" -type f | wc -l) files, $(du -sb "$lo_dst" | cut -f1) B"
  else
    log "DRY-RUN would: copy $lo_src -> $lo_dst and strip it"
  fi
else
  log "soffice SKIPPED (no --lo-pack): documents would be an unproven family"
fi

# --- 4. optional ezc-pdf (pdfium sidecar) -----------------------------------
if [ -n "$pdfium_src" ]; then
  [ -f "$pdfium_src" ] || die "missing $pdfium_src"
  if [ "$dry" -eq 0 ]; then
    cp -f "$pdfium_src" "$bin_dir/ezc-pdf-$target_triple.exe"
    log "ezc-pdf $(stat -c%s "$bin_dir/ezc-pdf-$target_triple.exe") B"
  fi
else
  log "ezc-pdf SKIPPED (ARCHITECTURE 17.2 sidecar not built yet; PDF routes stay unshipped)"
fi

# --- 5. licences + size report + gate --------------------------------------
if [ "$dry" -eq 0 ]; then
  mkdir -p "$lic_dir"
  cp -f "$repo_root/docs/licenses/THIRD-PARTY-NOTICES.md" "$lic_dir/"
  cp -f "$repo_root/docs/LICENSES.md" "$lic_dir/EZ-CONVERT-DEPENDENCIES.md"
  [ ! -d "$vips_dst/licenses" ] || cp -a "$vips_dst/licenses" "$lic_dir/vips"
  [ ! -f "$lo_dst/README" ] || cp -f "$lo_dst/README" "$lic_dir/libreoffice-README" || true
  log "licenses $(find "$lic_dir" -type f | wc -l) files"

  : > "$manifest"
  for tree in "$bin_dir" "$eng_dir"; do
    [ -d "$tree" ] || continue
    ( cd "$(dirname "$tree")" && find "$(basename "$tree")" -type f -print0 |
        sort -z | xargs -0 -r sha256sum ) >> "$manifest"
  done
  log "manifest $manifest ($(wc -l < "$manifest") files)"

  log "gate: scanning the exact trees the bundler will ship"
  # Each engine tree is scanned from its own root: the inventory names files
  # relative to the artifact root (`licenses/GPL-3.0.txt` for the vips dist,
  # `program/**` for the LO pack), so scanning from `engines/` would invent a
  # prefix the inventory never promised.
  #
  # The externalBin staging names carry `-<triple>` because that is what Tauri
  # requires on disk; the bundler strips the suffix, so the file that reaches
  # the install directory is `ffmpeg.exe`/`ffprobe.exe` — the names the
  # inventory covers. The gate is therefore run over a projection of the final
  # names (hard links, no copy) rather than over the staging names.
  projection=$(mktemp -d)
  for exe in ffmpeg ffprobe; do
    ln "$bin_dir/$exe-$target_triple.exe" "$projection/$exe.exe"
  done
  for tree in "$projection" "$eng_dir"/*/; do
    [ -d "$tree" ] || continue
    bash "$repo_root/scripts/dist-license-gate.sh" --scan "${tree%/}"
  done
  rm -rf "$projection"

  # `resources/licenses` holds copies, not new artifacts: every file must be
  # byte-identical to one that is already attributed where it originates (the
  # vips dist's own `licenses/`, `docs/licenses/**`, `docs/LICENSES.md`). A
  # copied notice that matches no attributed source is unattributable content
  # and fails here, without inventing a second path convention in the inventory.
  known=$(mktemp)
  {
    find "$repo_root/docs/licenses" -type f -print
    printf '%s\n' "$repo_root/docs/LICENSES.md"
    find "$vips_dst/licenses" -type f -print 2>/dev/null
  } | xargs -r -d '\n' sha256sum | cut -d' ' -f1 | sort -u > "$known"
  lic_fail=0
  while IFS= read -r f; do
    h=$(sha256sum "$f" | cut -d' ' -f1)
    grep -qx "$h" "$known" || { log "FAIL licences: ${f#"$lic_dir"/} is not a copy of an attributed notice"; lic_fail=$((lic_fail + 1)); }
  done < <(find "$lic_dir" -type f)
  rm -f "$known"
  [ "$lic_fail" -eq 0 ] || die "licence folder has $lic_fail unattributed file(s)"

  log ""
  log "== bundle inputs =="
  du -sh "$bin_dir" "$eng_dir" "$lic_dir" 2>/dev/null || true
  du -sb "$bin_dir" "$eng_dir" "$lic_dir" | awk '{n+=$1} END {printf "staged total: %.1f MiB\n", n/1048576}'
  log "next: tauri build --target $target_triple  (docs/PACKAGING-WINDOWS.md)"
else
  log "DRY-RUN complete; nothing written"
fi
