#!/bin/bash
# C2 discovery round 3: audio + video argv behaviour on the LGPL-only ffmpeg.
set -uo pipefail
ROOT=/opt/data/profiles/wright/cache/scratch/eng
B=$ROOT/ffmpeg-master-latest-linux64-lgpl/bin
FF=$B/ffmpeg
FP=$B/ffprobe
W=$ROOT/w3
REPO=/opt/data/kanban/boards/ezconvert/workspaces/t_dc07a72e/repo
rm -rf "$W"; mkdir -p "$W"
cp "$REPO"/fixtures/* "$W"/ 2>/dev/null
cd "$W" || exit 1

say() { printf '\n### %s\n' "$*"; }
run() { printf '$ %s\n' "$*"; "$@" 2>&1; printf 'rc=%d\n' $?; }
ainfo() { "$FP" -v error -select_streams a:0 -show_entries stream=codec_name,sample_rate,channels,sample_fmt,bit_rate -show_entries format=duration -of default=nw=1 "$1" 2>&1 | sed 's/^/    /'; }
vinfo() { "$FP" -v error -select_streams v:0 -show_entries stream=codec_name,width,height,r_frame_rate,avg_frame_rate,nb_frames,pix_fmt -show_entries format=duration -of default=nw=1 "$1" 2>&1 | sed 's/^/    /'; }

say "engine identity"
"$FF" -version 2>&1 | head -2 | sed 's/^/  /'
printf '  enable-gpl occurrences: %s\n' "$("$FF" -version 2>&1 | grep -o 'enable-gpl' | wc -l)"
printf '  libx264/libx265 in config: %s\n' "$("$FF" -version 2>&1 | grep -oE 'enable-libx26[45]' | wc -l)"
printf '  ffprobe: %s\n' "$("$FP" -version 2>&1 | head -1)"

say "source fixture parameters"
for f in sample.mp3 sample.wav sample.flac sample.aac sample.m4a sample.ogg sample.opus; do
  printf '  -- %s\n' "$f"; ainfo "$f"
done
for f in sample.mp4 sample.mkv sample.mov sample.webm; do
  printf '  -- %s\n' "$f"; vinfo "$f"
done

say "A1 mp3 -> wav (explicit -ar/-sample_fmt)"
run "$FF" -nostdin -y -i sample.mp3 -vn -map_metadata -1 -c:a pcm_s16le -ar 44100 -sample_fmt s16 a1.wav
ainfo a1.wav

say "A2 mp3 -> wav with -ar 48000 + s16"
run "$FF" -nostdin -y -i sample.mp3 -vn -map_metadata -1 -c:a pcm_s16le -ar 48000 -sample_fmt s16 a2.wav
ainfo a2.wav

say "A3 wav -> mp3 (libmp3lame, s16p)"
run "$FF" -nostdin -y -i sample.wav -vn -map_metadata -1 -c:a libmp3lame -b:a 192k -ar 44100 -sample_fmt s16p a3.mp3
ainfo a3.mp3

say "A4 wav -> flac (s16)"
run "$FF" -nostdin -y -i sample.wav -vn -map_metadata -1 -c:a flac -ar 44100 -sample_fmt s16 a4.flac
ainfo a4.flac

say "A5 wav -> m4a (aac + ipod muxer + faststart)"
run "$FF" -nostdin -y -i sample.wav -vn -map_metadata -1 -c:a aac -b:a 192k -ar 44100 -sample_fmt fltp -movflags +faststart -f ipod a5.m4a
ainfo a5.m4a

say "A6 wav -> aac (adts)"
run "$FF" -nostdin -y -i sample.wav -vn -map_metadata -1 -c:a aac -b:a 192k -ar 44100 -sample_fmt fltp -f adts a6.aac
ainfo a6.aac

say "A7 wav -> ogg (libvorbis)"
run "$FF" -nostdin -y -i sample.wav -vn -map_metadata -1 -c:a libvorbis -q:a 5 -ar 44100 -sample_fmt fltp a7.ogg
ainfo a7.ogg

say "A8 wav -> opus at source rate 44100 (expect forced 48000 or an error)"
run "$FF" -nostdin -y -i sample.wav -vn -map_metadata -1 -c:a libopus -b:a 128k -ar 44100 -sample_fmt s16 a8.ogg
ainfo a8.ogg

say "A9 wav -> opus explicit 48000"
run "$FF" -nostdin -y -i sample.wav -vn -map_metadata -1 -c:a libopus -b:a 128k -ar 48000 -sample_fmt s16 -f ogg a9.opus
ainfo a9.opus

say "A10 flac -> mp3 (96 kHz source probe: does -ar need clamping?)"
run "$FF" -nostdin -y -i sample.flac -vn -map_metadata -1 -c:a libmp3lame -b:a 192k -ar 44100 -sample_fmt s16p a10.mp3
ainfo a10.mp3

say "A11 opus source -> wav (48k source)"
run "$FF" -nostdin -y -i sample.opus -vn -map_metadata -1 -c:a pcm_s16le -ar 48000 -sample_fmt s16 a11.wav
ainfo a11.wav

say "A12 m4a -> mp3"
run "$FF" -nostdin -y -i sample.m4a -vn -map_metadata -1 -c:a libmp3lame -b:a 192k -ar 44100 -sample_fmt s16p a12.mp3
ainfo a12.mp3

say "A13 ogg -> flac"
run "$FF" -nostdin -y -i sample.ogg -vn -map_metadata -1 -c:a flac -ar 44100 -sample_fmt s16 a13.flac
ainfo a13.flac

say "A14 aac(adts) -> wav"
run "$FF" -nostdin -y -i sample.aac -vn -map_metadata -1 -c:a pcm_s16le -ar 44100 -sample_fmt s16 a14.wav
ainfo a14.wav

say "A15 no -sample_fmt on pcm target: pcm ignores it?"
run "$FF" -nostdin -y -i sample.mp3 -vn -c:a pcm_s16le -ar 44100 a15.wav
ainfo a15.wav

say "A16 tags: write a tagged+cover mp3, then re-encode and see what ffmpeg itself does with tags"
run "$FF" -nostdin -y -f lavfi -i "sine=frequency=440:duration=1.2" -i sample.jpg -map 0:a -map 1:v -c:a libmp3lame -b:a 128k -c:v mjpeg -disposition:v attached_pic -metadata title="C2 Tag Probe" -metadata artist="EZ Convert" -metadata album="Fixtures" -metadata date=2026 tagged.mp3
printf '  ffprobe format tags:\n'
"$FP" -v error -show_entries format_tags -of default=nw=1 tagged.mp3 2>&1 | sed 's/^/    /'
printf '  streams:\n'
"$FP" -v error -show_entries stream=index,codec_name,codec_type,disposition -of default=nw=1 tagged.mp3 2>&1 | sed 's/^/    /'

say "A17 ffmpeg re-encode of tagged.mp3 -> mp3 with -map_metadata -1 (tags dropped, lofty's job)"
run "$FF" -nostdin -y -i tagged.mp3 -vn -map_metadata -1 -c:a libmp3lame -b:a 192k -ar 44100 -sample_fmt s16p a17.mp3
"$FP" -v error -show_entries format_tags -of default=nw=1 a17.mp3 2>&1 | sed 's/^/    /'
printf '  (empty above = no tags, as designed)\n'

say "A18 ffmpeg re-encode with -map_metadata 0 (what tags survive natively?)"
run "$FF" -nostdin -y -i tagged.mp3 -vn -map_metadata 0 -c:a libmp3lame -b:a 192k -ar 44100 -sample_fmt s16p a18.mp3
"$FP" -v error -show_entries format_tags -of default=nw=1 a18.mp3 2>&1 | sed 's/^/    /'

say "V1 mp4 -> mp4 (libopenh264 + aac, CFR)"
run "$FF" -nostdin -y -i sample.mp4 -map 0:v:0 -map 0:a:0? -fps_mode cfr -r 25 -c:v libopenh264 -b:v 1500k -pix_fmt yuv420p -c:a aac -b:a 128k -ar 44100 -sample_fmt fltp -movflags +faststart v1.mp4
vinfo v1.mp4

say "V2 mp4 -> webm (libvpx-vp9 + libopus 48k)"
run "$FF" -nostdin -y -i sample.mp4 -map 0:v:0 -map 0:a:0? -fps_mode cfr -r 25 -c:v libvpx-vp9 -b:v 0 -crf 34 -pix_fmt yuv420p -c:a libopus -b:a 96k -ar 48000 -sample_fmt s16 v2.webm
vinfo v2.webm

say "V3 mp4 -> mkv (h264 + aac)"
run "$FF" -nostdin -y -i sample.mp4 -map 0:v:0 -map 0:a:0? -fps_mode cfr -r 25 -c:v libopenh264 -b:v 1500k -pix_fmt yuv420p -c:a aac -b:a 128k -ar 44100 -sample_fmt fltp v3.mkv
vinfo v3.mkv

say "V4 mp4 -> mov (h264 + aac + faststart)"
run "$FF" -nostdin -y -i sample.mp4 -map 0:v:0 -map 0:a:0? -fps_mode cfr -r 25 -c:v libopenh264 -b:v 1500k -pix_fmt yuv420p -c:a aac -b:a 128k -ar 44100 -sample_fmt fltp -movflags +faststart v4.mov
vinfo v4.mov

say "V5 webm -> mp4 (vp9 decode -> h264 encode)"
run "$FF" -nostdin -y -i sample.webm -map 0:v:0 -map 0:a:0? -fps_mode cfr -r 25 -c:v libopenh264 -b:v 1500k -pix_fmt yuv420p -c:a aac -b:a 128k -ar 48000 -sample_fmt fltp -movflags +faststart v5.mp4
vinfo v5.mp4

say "V6 mkv/mov inputs -> mp4"
run "$FF" -nostdin -y -i sample.mkv -map 0:v:0 -map 0:a:0? -fps_mode cfr -r 25 -c:v libopenh264 -b:v 1500k -pix_fmt yuv420p -c:a aac -b:a 128k -ar 44100 -sample_fmt fltp -movflags +faststart v6.mkv.mp4
run "$FF" -nostdin -y -i sample.mov -map 0:v:0 -map 0:a:0? -fps_mode cfr -r 25 -c:v libopenh264 -b:v 1500k -pix_fmt yuv420p -c:a aac -b:a 128k -ar 44100 -sample_fmt fltp -movflags +faststart v6.mov.mp4
vinfo v6.mkv.mp4
vinfo v6.mov.mp4

say "V7 VFR source synthesis + detection"
run "$FF" -nostdin -y -f lavfi -i "testsrc=size=320x240:rate=30:duration=3" -vf "setpts='if(lt(N,45),N/30/TB,(N-45)*2/30/TB)'" -fps_mode vfr -c:v mpeg4 -q:v 5 vfr.mkv
vinfo vfr.mkv
printf '  r_frame_rate vs avg_frame_rate differ = VFR marker\n'

say "V8 VFR -> mp4 with -fps_mode cfr (forced constant rate)"
run "$FF" -nostdin -y -i vfr.mkv -map 0:v:0 -fps_mode cfr -r 30 -c:v libopenh264 -b:v 1200k -pix_fmt yuv420p v8.mp4
vinfo v8.mp4
printf '  frame count check:\n'
"$FP" -v error -count_frames -select_streams v:0 -show_entries stream=nb_read_frames -of default=nw=1 v8.mp4 2>&1 | sed 's/^/    /'

say "V9 VFR -> mp4 without -fps_mode (control: what happens by default)"
run "$FF" -nostdin -y -i vfr.mkv -map 0:v:0 -c:v libopenh264 -b:v 1200k -pix_fmt yuv420p v9.mp4
vinfo v9.mp4

say "V10 rotation: make a display-matrix rotated source and transcode"
run "$FF" -nostdin -y -f lavfi -i "testsrc=size=320x240:rate=25:duration=2" -c:v mpeg4 -q:v 5 rot0.mkv
run "$FF" -nostdin -y -display_rotation 90 -i rot0.mkv -c copy rot90.mkv
printf '  rot90 side data:\n'
"$FP" -v error -select_streams v:0 -show_entries stream_side_data=rotation -of default=nw=1 rot90.mkv 2>&1 | sed 's/^/    /'
run "$FF" -nostdin -y -i rot90.mkv -map 0:v:0 -fps_mode cfr -r 25 -c:v libopenh264 -b:v 1200k -pix_fmt yuv420p v10.mp4
vinfo v10.mp4
printf '  (240x320 output = rotation applied by ffmpeg; 320x240 = not applied)\n'

say "V11 trim: -ss 0.5 -to 1.5 on a 2 s clip"
run "$FF" -nostdin -y -ss 0.5 -to 1.5 -i sample.mp4 -map 0:v:0 -map 0:a:0? -fps_mode cfr -r 25 -c:v libopenh264 -b:v 1500k -pix_fmt yuv420p -c:a aac -b:a 128k -ar 44100 -sample_fmt fltp v11.mp4
vinfo v11.mp4

say "V12 resize: scale=160:-2"
run "$FF" -nostdin -y -i sample.mp4 -map 0:v:0 -map 0:a:0? -vf "scale=160:-2:flags=lanczos" -fps_mode cfr -r 25 -c:v libopenh264 -b:v 800k -pix_fmt yuv420p -c:a aac -b:a 128k -ar 44100 -sample_fmt fltp v12.mp4
vinfo v12.mp4

say "V13 still image -> gif (vips-normalised PNG input, palettegen/paletteuse)"
run "$FF" -nostdin -y -i sample.png -vf "split[a][b];[a]palettegen=stats_mode=single[p];[b][p]paletteuse=dither=bayer:bayer_scale=3" -loop 0 v13.gif
ls -l v13.gif
"$FP" -v error -show_entries stream=codec_name,width,height,nb_frames -of default=nw=1 v13.gif 2>&1 | sed 's/^/    /'

say "V14 video -> gif (fps + scale + palette)"
run "$FF" -nostdin -y -i sample.mp4 -vf "fps=12,scale=160:-1:flags=lanczos,split[a][b];[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=3" -loop 0 v14.gif
ls -l v14.gif
"$FP" -v error -show_entries stream=codec_name,width,height,nb_frames -of default=nw=1 v14.gif 2>&1 | sed 's/^/    /'

say "V15 video stream copy safety: does '-c:v copy' preserve rotation? (control for the contract)"
run "$FF" -nostdin -y -i rot90.mkv -map 0:v:0 -c copy v15.mkv
"$FP" -v error -select_streams v:0 -show_entries stream_side_data=rotation -of default=nw=1 v15.mkv 2>&1 | sed 's/^/    /'

say "V16 broken input: truncated/corrupt mp4 must fail loudly"
run "$FF" -nostdin -y -i truncated.jpg -c:a pcm_s16le v16.wav
ls -l v16.wav 2>&1

say "V17 zero-byte and missing inputs"
: > empty.mp4
run "$FF" -nostdin -y -i empty.mp4 -c:v libopenh264 v17.mp4
ls -l v17.mp4 2>&1
run "$FF" -nostdin -y -i nope.mp4 -c:v libopenh264 v17b.mp4
ls -l v17b.mp4 2>&1

echo
echo "DISCOVERY-3-END"
