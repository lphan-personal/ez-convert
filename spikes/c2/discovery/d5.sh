#!/bin/bash
# C2 discovery round 5: trim semantics on a 2 s clip + the audio questions d4 missed.
set -uo pipefail
ROOT=/opt/data/profiles/wright/cache/scratch/eng
P=$ROOT/prefix
B=$ROOT/ffmpeg-master-latest-linux64-lgpl/bin
FF=$B/ffmpeg
FP=$B/ffprobe
V=$P/usr/bin/vips
export LD_LIBRARY_PATH=$P/usr/lib/x86_64-linux-gnu
W=$ROOT/w5
REPO=/opt/data/kanban/boards/ezconvert/workspaces/t_dc07a72e/repo
rm -rf "$W"; mkdir -p "$W"
cp "$REPO"/fixtures/* "$W"/ 2>/dev/null
cp "$ROOT"/w3/tagged.mp3 "$W"/ 2>/dev/null
cd "$W" || exit 1
FFQ=("$FF" -nostdin -y -hide_banner -loglevel error)

say() { printf '\n### %s\n' "$*"; }
run() { printf '$ %s\n' "$*"; "$@" 2>&1; printf 'rc=%d\n' $?; }
vlen() { "$FP" -v error -show_entries format=duration -show_entries stream=nb_frames,r_frame_rate,avg_frame_rate,width,height -of default=nw=1 "$1" 2>&1 | sed 's/^/    /'; }

say "S0 make a 2 s / 25 fps source clip"
run "${FFQ[@]}" -f lavfi -i "testsrc=size=320x240:rate=25:duration=2" -c:v mpeg4 -q:v 5 clip2s.mkv
vlen clip2s.mkv

say "T1 input-side -ss 0.5 -to 1.5 (is -to absolute or relative to -ss?)"
run "${FFQ[@]}" -ss 0.5 -to 1.5 -i clip2s.mkv -map 0:v:0 -fps_mode cfr -r 25 -c:v libopenh264 -b:v 1200k -pix_fmt yuv420p t1.mp4
vlen t1.mp4

say "T2 input-side -ss 0.5 -t 1.0 (duration form)"
run "${FFQ[@]}" -ss 0.5 -t 1.0 -i clip2s.mkv -map 0:v:0 -fps_mode cfr -r 25 -c:v libopenh264 -b:v 1200k -pix_fmt yuv420p t2.mp4
vlen t2.mp4

say "T3 output-side -ss 0.5 -to 1.5 (after -i)"
run "${FFQ[@]}" -i clip2s.mkv -ss 0.5 -to 1.5 -map 0:v:0 -fps_mode cfr -r 25 -c:v libopenh264 -b:v 1200k -pix_fmt yuv420p t3.mp4
vlen t3.mp4

say "A1 mp3 with cover art -> wav: does -vn drop the cover cleanly?"
run "${FFQ[@]}" -i tagged.mp3 -vn -map_metadata -1 -c:a pcm_s16le -ar 44100 -sample_fmt s16 a1.wav
"$FP" -v error -show_entries stream=index,codec_type -of default=nw=1 a1.wav | sed 's/^/    /'

say "A2 mp3 with cover art -> flac, tags off"
run "${FFQ[@]}" -i tagged.mp3 -vn -map_metadata -1 -c:a flac -ar 44100 -sample_fmt s16 a2.flac
"$FP" -v error -show_entries format_tags -of default=nw=1 a2.flac | sed 's/^/    /'

say "A3 libmp3lame accepted sample rates"
for r in 8000 22050 44100 48000; do
  printf '$ -ar %s\n' "$r"
  "${FFQ[@]}" -i sample.wav -vn -map_metadata -1 -c:a libmp3lame -b:a 128k -ar "$r" -sample_fmt s16p "a3-$r.mp3" 2>&1 | sed 's/^/    /'
  printf '  rc=%d out: ' $?
  "$FP" -v error -show_entries stream=sample_rate -of default=nw=1 "a3-$r.mp3" 2>&1 | tr '\n' ' '; echo
done

say "A4 libopus with -ar 44100 (measured failure) — full error text"
"${FFQ[@]}" -i sample.wav -vn -c:a libopus -b:a 128k -ar 44100 -sample_fmt s16 bad.ogg 2>&1 | head -4 | sed 's/^/    /'
printf '  rc=%d\n' $?

say "A5 -sample_fmt s16p vs fltp on libmp3lame: same size?"
"${FFQ[@]}" -i sample.wav -vn -c:a libmp3lame -b:a 128k -ar 44100 -sample_fmt s16p a5a.mp3
"${FFQ[@]}" -i sample.wav -vn -c:a libmp3lame -b:a 128k -ar 44100 -sample_fmt fltp a5b.mp3
ls -l a5a.mp3 a5b.mp3 | sed 's/^/    /'

say "A6 channels preserved without -ac"
"${FFQ[@]}" -f lavfi -i "sine=frequency=440:duration=0.5" -ac 2 -c:a pcm_s16le stereo.wav
"$FP" -v error -show_entries stream=channels -of default=nw=1 stereo.wav | sed 's/^/    /'
"${FFQ[@]}" -i stereo.wav -vn -c:a libmp3lame -b:a 128k -ar 44100 -sample_fmt s16p stereo.mp3
"$FP" -v error -show_entries stream=channels,sample_rate -of default=nw=1 stereo.mp3 | sed 's/^/    /'

say "A7 96 kHz wav -> mp3 with -ar 96000 (does lame reject?)"
"${FFQ[@]}" -f lavfi -i "sine=frequency=440:duration=0.5" -ar 96000 -c:a pcm_s24le hi.wav
"$FP" -v error -show_entries stream=sample_rate,bits_per_raw_sample -of default=nw=1 hi.wav | sed 's/^/    /'
run "${FFQ[@]}" -i hi.wav -vn -c:a libmp3lame -b:a 128k -ar 96000 -sample_fmt s16p hi.mp3
run "${FFQ[@]}" -i hi.wav -vn -c:a flac -ar 96000 -sample_fmt s32 hi.flac
"$FP" -v error -show_entries stream=sample_rate,sample_fmt -of default=nw=1 hi.flac | sed 's/^/    /'
run "${FFQ[@]}" -i hi.wav -vn -c:a libopus -b:a 128k -ar 48000 -sample_fmt s16 hi.opus
"$FP" -v error -show_entries stream=sample_rate -of default=nw=1 hi.opus | sed 's/^/    /'

say "A8 wav 24-bit -> wav s16le (bit-depth reduction is explicit)"
run "${FFQ[@]}" -i hi.wav -vn -c:a pcm_s16le -ar 96000 -sample_fmt s16 a8.wav
"$FP" -v error -show_entries stream=codec_name,sample_fmt,sample_rate -of default=nw=1 a8.wav | sed 's/^/    /'

say "A9 does ffmpeg accept an image as an audio input (must fail, not produce garbage)"
run "${FFQ[@]}" -i sample.jpg -vn -c:a pcm_s16le a9.wav
ls -l a9.wav 2>&1

echo
echo "DISCOVERY-5-END"
