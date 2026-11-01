#!/bin/bash
# C2 discovery run 1: image argv behaviour on a real libvips CLI.
# Everything printed here is tool output; nothing is inferred.
set -uo pipefail
ROOT=/opt/data/profiles/wright/cache/scratch/eng
P=$ROOT/prefix
export LD_LIBRARY_PATH=$P/usr/lib/x86_64-linux-gnu
export VIPS_PLUGINDIR=$P/usr/lib/x86_64-linux-gnu/vips-modules-8.16
V=$P/usr/bin/vips
H=$P/usr/bin/vipsheader
W=$ROOT/w1
REPO=/opt/data/kanban/boards/ezconvert/workspaces/t_dc07a72e/repo
mkdir -p "$W"
cp "$REPO"/fixtures/sample.* "$W"/ 2>/dev/null
cp /opt/data/converter/build/windows/fixtures/orient6.jpg "$W"/exif_icc.jpg
cp /opt/data/converter/build/windows/fixtures/orient6.heic "$W"/exif_icc.heic
cd "$W" || exit 1

say() { printf '\n### %s\n' "$*"; }
run() { printf '$ %s\n' "$*"; "$@" 2>&1; printf 'rc=%d\n' $?; }

say "engine"
run "$V" --version
"$V" --vips-config 2>&1 | grep -Ei "exif|ICC|libjpeg|PNG load|WebP|TIFF|HEIF|nsgif|cgif|rsvg|bmp|Magick" | sed 's/^/  /'

say "source metadata (exif_icc.jpg)"
run "$H" -a exif_icc.jpg

say "V-a jpg -> png, named loader, fail-on error (EXIF/ICC survival)"
run "$V" jpegload exif_icc.jpg out_a.png --fail-on error
run "$H" -a out_a.png

say "V-b jpg -> png via plain copy (control)"
run "$V" copy exif_icc.jpg out_b.png
run "$H" -a out_b.png

say "V-c truncated jpg: named loader + fail-on error must fail, no output"
run "$V" jpegload ../sample/truncated.jpg out_c.png --fail-on error
ls -l out_c.png 2>&1

say "V-d truncated jpg via plain copy (the silent-garbage control)"
run "$V" copy ../sample/truncated.jpg out_d.png
ls -l out_d.png 2>&1

say "V-e strip probe: does --strip remove EXIF/ICC?"
run "$V" jpegload exif_icc.jpg out_e.jpg --fail-on error --strip
run "$H" -a out_e.jpg
run "$V" jpegload exif_icc.jpg out_e2.jpg --fail-on error
run "$H" -a out_e2.jpg

say "V-f quality: jpegload -> jpegsave --Q 80"
run "$V" jpegload exif_icc.jpg out_f.jpg --fail-on error --Q 80
ls -l out_f.jpg
run "$H" -a out_f.jpg

say "V-g webp save with Q"
run "$V" jpegload exif_icc.jpg out_g.webp --fail-on error --Q 80
ls -l out_g.webp
run "$H" -a out_g.webp

say "V-h tiff save (deflate) + metadata"
run "$V" jpegload exif_icc.jpg out_h.tif --fail-on error --compression deflate
ls -l out_h.tif
run "$H" -a out_h.tif

say "V-i png save options (compression 9)"
run "$V" jpegload exif_icc.jpg out_i.png --fail-on error --compression 9
ls -l out_i.png

say "V-j avif save from jpg (heifsave av1)"
run "$V" jpegload exif_icc.jpg out_j.avif --fail-on error --compression av1 --Q 60 --effort 6 --bitdepth 8 --keep all
ls -l out_j.avif
run "$H" -a out_j.avif

say "V-k avif -> jpg"
run "$V" jpegload out_j.avif out_k.jpg --fail-on error --Q 95
run "$H" -a out_k.jpg

say "V-l heic decode -> jpg"
run "$V" jpegload exif_icc.heic out_l.jpg --fail-on error
run "$H" -a out_l.jpg

say "V-m bmp load / save"
run "$V" magickload sample.bmp out_m.png --fail-on error
ls -l out_m.png
run "$V" jpegload exif_icc.jpg out_m.bmp --Q 95
ls -l out_m.bmp
run "$V" magickload out_m.bmp out_m2.png
run "$H" out_m2.png

say "V-n gif load / save"
run "$V" gifload sample.gif out_n.png
run "$H" out_n.png
run "$V" jpegload exif_icc.jpg out_n.gif
ls -l out_n.gif

say "V-o svg load"
run "$V" svgload sample.svg out_o.png --fail-on error
run "$H" out_o.png

say "V-p tiff input: named loader + fail-on, and per-loader fail-on accepted?"
run "$V" tiffload sample.tif out_p.png --fail-on error
run "$H" out_p.png

say "V-q does copy accept --fail-on? (ARCHITECTURE says no)"
run "$V" copy sample.png out_q.png --fail-on error

say "V-r autorot jpeg (orientation 6)"
run "$V" autorot exif_icc.jpg out_r.jpg --Q 95
run "$H" -a out_r.jpg

say "V-s resize: thumbnail vs resize"
run "$V" resize sample.png out_s.png 0.5
run "$H" out_s.png
run "$V" thumbnail sample.png out_s2.png 100
run "$H" out_s2.png

say "V-t strip on png output (does --strip work on pngsave?)"
run "$V" jpegload exif_icc.jpg out_t.png --fail-on error --strip
run "$H" -a out_t.png
echo
echo "DISCOVERY-1-END"
