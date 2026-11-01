#!/bin/bash
# C2 discovery round 2: vips CLI filename-option syntax, savers, formats.
set -uo pipefail
ROOT=/opt/data/profiles/wright/cache/scratch/eng
P=$ROOT/prefix
export LD_LIBRARY_PATH=$P/usr/lib/x86_64-linux-gnu
export VIPS_PLUGINDIR=$P/usr/lib/x86_64-linux-gnu/vips-modules-8.16
V=$P/usr/bin/vips
H=$P/usr/bin/vipsheader
W=$ROOT/w2
REPO=/opt/data/kanban/boards/ezconvert/workspaces/t_dc07a72e/repo
rm -rf "$W"; mkdir -p "$W"
cp "$REPO"/fixtures/* "$W"/ 2>/dev/null
cp /opt/data/converter/build/windows/fixtures/orient6.jpg "$W"/exif_icc.jpg
cp /opt/data/converter/build/windows/fixtures/orient6.heic "$W"/exif_icc.heic
cd "$W" || exit 1

say() { printf '\n### %s\n' "$*"; }
run() { printf '$ %s\n' "$*"; "$@" 2>&1; printf 'rc=%d\n' $?; }
meta() { "$H" -a "$1" 2>&1 | grep -Ei "icc-profile-data|exif-ifd0-Orientation|^orientation|width:|height:|bands:|vips-loader|n-pages|page-height" | sed 's/^/    /'; }

say "which candidates carry an ICC profile?"
for f in *.jpg *.heic /opt/data/converter/build/windows/fixtures/*.jpg; do
  [ -f "$f" ] || continue
  n=$("$H" -a "$f" 2>/dev/null | grep -c "icc-profile-data")
  printf '  %-46s icc-profile-data lines=%s\n' "$f" "$n"
done

say "V1 bracket syntax: loader option on input filename"
run "$V" copy 'exif_icc.jpg[fail-on=error]' o1.png
meta o1.png

say "V2 bracket syntax: saver option on output filename (Q)"
run "$V" copy exif_icc.jpg 'o2.jpg[Q=80]'
ls -l o2.jpg; meta o2.jpg
run "$V" copy exif_icc.jpg 'o2b.jpg[Q=20]'
ls -l o2b.jpg

say "V3 bracket syntax: strip on output"
run "$V" copy exif_icc.jpg 'o3.jpg[strip]'
meta o3.jpg
run "$V" copy exif_icc.jpg 'o3.png[strip]'
meta o3.png

say "V4 bracket: png compression"
run "$V" copy exif_icc.jpg 'o4.png[compression=9]'
ls -l o4.png

say "V5 bracket: webp Q"
run "$V" copy exif_icc.jpg 'o5.webp[Q=80]'
ls -l o5.webp; meta o5.webp

say "V6 bracket: tiff compression"
run "$V" copy exif_icc.jpg 'o6.tif[compression=deflate]'
ls -l o6.tif; meta o6.tif

say "V7 bracket: avif (heifsave) compression/Q/effort/bitdepth"
run "$V" copy exif_icc.jpg 'o7.avif[compression=av1,Q=60,effort=6,bitdepth=8,keep=all]'
ls -l o7.avif; meta o7.avif

say "V8 truncated jpg with loader fail-on in bracket form (must fail, no output)"
run "$V" copy 'truncated.jpg[fail-on=error]' o8.png
ls -l o8.png 2>&1

say "V9 truncated jpg with plain copy (silent-garbage control)"
run "$V" copy truncated.jpg o9.png
ls -l o9.png 2>&1

say "V10 truncated png both ways"
run "$V" copy 'truncated.png[fail-on=error]' o10.png
run "$V" copy truncated.png o10b.png
ls -l o10b.png 2>&1

say "V11 heic decode (heifload) with fail-on"
run "$V" copy 'exif_icc.heic[fail-on=error]' o11.jpg
meta o11.jpg

say "V12 avif decode -> jpg"
run "$V" copy 'o7.avif[fail-on=error]' 'o12.jpg[Q=95]'
meta o12.jpg

say "V13 bmp load: is there a native bmp loader in this build?"
"$V" -l 2>/dev/null | grep -i "bmp" | sed 's/^/  /'
run "$V" copy sample.bmp o13.png
ls -l o13.png 2>&1
meta o13.png

say "V14 bmp save attempt"
run "$V" copy exif_icc.jpg o14.bmp
ls -l o14.bmp 2>&1

say "V15 gif input: pages?"
"$H" -a sample.gif 2>&1 | grep -Ei "n-pages|page-height|width|height" | sed 's/^/  /'
run "$V" copy sample.gif o15.png
meta o15.png

say "V16 gif output (cgif in this build; shipping dist has no cgif)"
run "$V" copy exif_icc.jpg o16.gif
ls -l o16.gif 2>&1

say "V17 svg input"
run "$V" copy sample.svg o17.png
meta o17.png

say "V18 autorot then save with Q (bracket)"
run "$V" autorot 'exif_icc.jpg[fail-on=error]' 'o18.jpg[Q=95]'
meta o18.jpg

say "V19 resize with bracket saver options"
run "$V" resize 'exif_icc.jpg[fail-on=error]' 'o19.png[compression=9]' 0.25
meta o19.png
run "$V" resize sample.png 'o19b.webp[Q=80]' 2
meta o19b.webp

say "V20 jpeg -> jpg (same family) with strip + Q"
run "$V" copy 'exif_icc.jpg[fail-on=error]' 'o20.jpg[Q=90,strip]'
meta o20.jpg
ls -l exif_icc.jpg o20.jpg

say "V21 does the loader honour fail-on for a valid file (no regression)?"
run "$V" copy 'sample.tif[fail-on=error]' o21.png
meta o21.png

say "V22 tiff -> jpg with Q"
run "$V" copy 'sample.tif[fail-on=error]' 'o22.jpg[Q=92]'
ls -l o22.jpg

say "V23 png -> tiff, then tiff -> png roundtrip"
run "$V" copy 'sample.png[fail-on=error]' 'o23.tif[compression=deflate]'
run "$V" copy 'o23.tif[fail-on=error]' o23b.png
ls -l o23.tif o23b.png

say "V24 missing input behaviour"
run "$V" copy 'nope.jpg[fail-on=error]' o24.png
ls -l o24.png 2>&1

say "V25 directory as input"
run "$V" copy '. ' o25.png

echo
echo "DISCOVERY-2-END"
