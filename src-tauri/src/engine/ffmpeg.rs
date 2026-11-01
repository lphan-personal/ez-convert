//! `engine/ffmpeg.rs` — the ffmpeg argv builder (audio, video, GIF palette).
//!
//! Pure: argv data only. Every number here comes from a measurement in
//! `evidence/c2-media.md`:
//!
//! - **`-ar` is always explicit and clamped to the encoder's measured set.**
//!   libmp3lame rejects 96 kHz (`Specified sample rate 96000 is not supported`,
//!   rc=234); libopus accepts only 48/24 kHz. Leaving the rate to ffmpeg's
//!   default is how sample-rate drift happens silently.
//! - **`-sample_fmt` is always explicit.** PCM is `s16`/`s32`, AAC and Vorbis are
//!   `fltp`, MP3 is `s16p`, Opus is `s16`.
//! - **Video is forced to CFR**: `-fps_mode cfr -r <source rate>`. Measured on a
//!   variable-timestamp source: with the flag the output is 89 frames at a
//!   constant 30 fps; without it the same input yields 68 frames at an average
//!   2040/89 fps — a file that stutters in every player.
//! - **Trim is a fast seek plus an exact length**: `-ss` *before* `-i` and `-t`
//!   *after* it. Measured on a 2 s / 25 fps clip: 25 frames, exactly 1.000 s.
//!   `-to` before `-i` is interpreted relative to the seek point and gave
//!   26 frames / 1.040 s.
//! - **No GPL encoders.** The shipped build has no libx264/libx265; H.264 is
//!   OpenH264 (`-c:v libopenh264`), VP9 is `libvpx-vp9`, audio is the native
//!   `aac` encoder, `libopus`, `libvorbis`, `libmp3lame`, `flac`.
//! - **Tags are dropped on purpose** (`-map_metadata -1`) and re-applied
//!   in-process by `tags.rs` (lofty). Measured: `-map_metadata 0` carries only
//!   the tags ffmpeg happens to understand.

use super::plan::{Arg, PathOpts, PathRef, Trim};
use crate::model::types::TargetFormat;

// ---------------------------------------------------------------------------
// Protocol guard
// ---------------------------------------------------------------------------

/// The protocols an ffmpeg/ffprobe input may reach.
///
/// `file` is the admitted path itself. `crypto` and `data` are what this ffmpeg
/// build already assumes for a plain file input (measured: `[file @ …] Setting
/// default whitelist 'file,crypto,data'`), so naming all three changes nothing
/// for a well-formed job. What it does change is the failure mode when a demuxer
/// opens something *the container asked for* rather than something we admitted:
/// `http`, `tcp`, `udp`, `rtmp`, `concat`, `subfile`, `pipe` are refused inside
/// libavformat instead of being fetched. That keeps the property
/// `proc::admit` guarantees (absolute, existing, regular file) true for every
/// nested open, not just the outermost one.
pub const INPUT_PROTOCOLS: &str = "file,crypto,data";

/// `-protocol_whitelist file,crypto,data` as a closed pair.
///
/// Both halves are `Arg::Word` — static text, never a filename, never a number
/// parsed out of the container — so no input can extend the list.
fn protocol_guard() -> [Arg; 2] {
    [
        Arg::Word("-protocol_whitelist"),
        Arg::Word(INPUT_PROTOCOLS),
    ]
}

// ---------------------------------------------------------------------------
// Filter graphs — numeric templates only, never user text
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    /// Single-frame GIF from an already-normalised PNG.
    PaletteFromStill,
    /// Video → GIF: drop the frame rate, cap the long edge, then palette.
    PaletteFromVideo { fps: u8, max_width: u32 },
    /// Video resize; height derived to keep the aspect ratio and stay even.
    ScaleWidth(u32),
}

impl Filter {
    pub fn render(self) -> String {
        match self {
            Filter::PaletteFromStill => {
                "split[a][b];[a]palettegen=stats_mode=single[p];[b][p]paletteuse=dither=bayer:bayer_scale=3"
                    .to_string()
            }
            Filter::PaletteFromVideo { fps, max_width } => format!(
                // `min(w\,iw)` keeps small sources from being upscaled; the
                // backslash escapes the comma inside the filter argument.
                "fps={},scale=min({}\\,iw):-1:flags=lanczos,split[a][b];\
                 [a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=3",
                fps.clamp(1, 60),
                max_width.clamp(64, 4096)
            ),
            Filter::ScaleWidth(w) => format!("scale={}:-2:flags=lanczos", w.clamp(16, 8192)),
        }
    }
}

// ---------------------------------------------------------------------------
// Audio
// ---------------------------------------------------------------------------

/// Measured sample rates each encoder accepts. From ffmpeg's own error text
/// (`libmp3lame` prints its list when handed 96 kHz) plus successful probes at
/// 8/22.05/44.1/48 kHz and 96 kHz for Vorbis/AAC.
const MP3_RATES: &[u32] = &[8000, 11025, 12000, 16000, 22050, 24000, 32000, 44100, 48000];
const OPUS_RATES: &[u32] = &[48000, 24000];
const AAC_RATES: &[u32] = &[
    8000, 11025, 12000, 16000, 22050, 24000, 32000, 44100, 48000, 64000, 88200, 96000,
];
const VORBIS_RATES: &[u32] = &[
    8000, 11025, 12000, 16000, 22050, 24000, 32000, 44100, 48000, 96000,
];

/// Nearest supported rate at or below the source; the smallest when the source
/// is below every supported value. Never invents a rate the encoder rejects.
pub fn clamp_rate(source: u32, allowed: &[u32]) -> u32 {
    if allowed.contains(&source) {
        return source;
    }
    allowed
        .iter()
        .copied()
        .filter(|r| *r <= source)
        .max()
        .unwrap_or_else(|| allowed.iter().copied().min().unwrap_or(48000))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioEncode {
    pub target: TargetFormat,
    pub codec: &'static str,
    pub bitrate_kbps: Option<u32>,
    pub sample_fmt: &'static str,
    pub rate: u32,
    /// Extra literals in front of the output path (`-f adts`, `-movflags …`).
    pub container: &'static [&'static str],
}

impl AudioEncode {
    /// `bits` is the source's `bits_per_raw_sample` (0 when unknown); it decides
    /// whether FLAC/WAV stay 24-bit or are honestly reduced to 16.
    pub fn for_target(
        to: TargetFormat,
        source_rate: u32,
        bits: u16,
        _quality: u8,
    ) -> Option<Self> {
        let wide = bits > 16;
        let enc = match to {
            TargetFormat::Mp3 => Self {
                target: to,
                codec: "libmp3lame",
                bitrate_kbps: Some(192),
                sample_fmt: "s16p",
                rate: clamp_rate(source_rate, MP3_RATES),
                container: &[],
            },
            TargetFormat::Wav => Self {
                target: to,
                codec: if wide { "pcm_s24le" } else { "pcm_s16le" },
                bitrate_kbps: None,
                sample_fmt: if wide { "s32" } else { "s16" },
                rate: source_rate,
                container: &[],
            },
            TargetFormat::Flac => Self {
                target: to,
                codec: "flac",
                bitrate_kbps: None,
                sample_fmt: if wide { "s32" } else { "s16" },
                rate: source_rate,
                container: &[],
            },
            TargetFormat::M4a => Self {
                target: to,
                codec: "aac",
                bitrate_kbps: Some(192),
                sample_fmt: "fltp",
                rate: clamp_rate(source_rate, AAC_RATES),
                container: &["-movflags", "+faststart", "-f", "ipod"],
            },
            TargetFormat::Aac => Self {
                target: to,
                codec: "aac",
                bitrate_kbps: Some(192),
                sample_fmt: "fltp",
                rate: clamp_rate(source_rate, AAC_RATES),
                container: &["-f", "adts"],
            },
            TargetFormat::Ogg => Self {
                target: to,
                codec: "libvorbis",
                // Vorbis is VBR; `-q:a` is its quality knob, not `-b:a`.
                bitrate_kbps: None,
                sample_fmt: "fltp",
                rate: clamp_rate(source_rate, VORBIS_RATES),
                container: &["-q:a", "5"],
            },
            TargetFormat::Opus => Self {
                target: to,
                codec: "libopus",
                bitrate_kbps: Some(128),
                sample_fmt: "s16",
                // Opus is a 48 kHz codec: the encoder refuses anything else, and
                // resampling up is lossless in practice. Not a clamp — a choice.
                rate: 48_000,
                container: &["-f", "ogg"],
            },
            _ => return None,
        };
        Some(enc)
    }
}

/// `-nostdin -y -hide_banner -loglevel error -i <in> -vn -map 0:a:0
///  -map_metadata -1 <codec args> <container args> <out>`
pub fn build_audio(enc: &AudioEncode, strip: bool) -> Vec<Arg> {
    let mut args = base();
    args.push(Arg::Word("-i"));
    args.push(Arg::Path(PathRef::Input, PathOpts::default()));
    // Audio only: a cover-art stream in the source must not become a video
    // stream in the output (measured: mp3 with attached_pic -> wav stays audio).
    args.push(Arg::Word("-vn"));
    args.push(Arg::Word("-map"));
    args.push(Arg::Word("0:a:0"));
    args.push(Arg::Word("-map_metadata"));
    args.push(Arg::Word("-1"));
    args.push(Arg::Word("-c:a"));
    args.push(Arg::Word(enc.codec));
    if let Some(kbps) = enc.bitrate_kbps {
        args.push(Arg::Word("-b:a"));
        args.push(Arg::Kbps(kbps));
    }
    for extra in enc.container {
        args.push(Arg::Word(extra));
    }
    args.push(Arg::Word("-ar"));
    args.push(Arg::Number(enc.rate));
    args.push(Arg::Word("-sample_fmt"));
    args.push(Arg::Word(enc.sample_fmt));
    let _ = strip; // stripping is a tag-merge decision, not an ffmpeg flag
    args.push(Arg::Path(PathRef::Output, PathOpts::default()));
    args
}

// ---------------------------------------------------------------------------
// Video
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoEncode {
    pub target: TargetFormat,
    pub vcodec: &'static str,
    /// Bitrate for the H.264 path; VP9 uses CQ rather than a fixed bitrate.
    pub vbitrate_kbps: Option<u32>,
    pub quality_args: &'static [&'static str],
    pub acodec: &'static str,
    pub abitrate_kbps: u32,
    pub audio_fmt: &'static str,
    pub audio_rate: u32,
    pub container: &'static [&'static str],
    pub movflags: bool,
}

impl VideoEncode {
    pub fn for_target(to: TargetFormat, audio_rate: u32) -> Option<Self> {
        let mp4ish = Self {
            target: to,
            vcodec: "libopenh264",
            vbitrate_kbps: Some(1500),
            quality_args: &[],
            acodec: "aac",
            abitrate_kbps: 128,
            audio_fmt: "fltp",
            audio_rate: clamp_rate(audio_rate, AAC_RATES),
            container: &[],
            movflags: true,
        };
        match to {
            TargetFormat::Mp4 => Some(Self {
                container: &["-f", "mp4"],
                ..mp4ish
            }),
            TargetFormat::Mov => Some(Self {
                container: &["-f", "mov"],
                ..mp4ish
            }),
            TargetFormat::Mkv => Some(Self {
                movflags: false,
                container: &["-f", "matroska"],
                ..mp4ish
            }),
            TargetFormat::Webm => Some(Self {
                target: to,
                vcodec: "libvpx-vp9",
                vbitrate_kbps: None,
                // Constrained quality: `-b:v 0 -crf 34` is the documented VP9
                // constant-quality form; a fixed bitrate alone gives mushy output.
                quality_args: &["-b:v", "0", "-crf", "34"],
                acodec: "libopus",
                abitrate_kbps: 96,
                audio_fmt: "s16",
                audio_rate: 48000,
                container: &["-f", "webm"],
                movflags: false,
            }),
            _ => None,
        }
    }
}

pub fn build_video(
    enc: &VideoEncode,
    fps_num: u32,
    fps_den: u32,
    has_audio: bool,
    resize_width: Option<u32>,
    trim: Option<Trim>,
    quality: u8,
) -> Vec<Arg> {
    let mut args = base();
    // Fast seek: -ss before -i. Exact length: -t after it (measured).
    if let Some(t) = trim {
        args.push(Arg::Word("-ss"));
        args.push(Arg::Millis(t.start_ms));
    }
    args.push(Arg::Word("-i"));
    args.push(Arg::Path(PathRef::Input, PathOpts::default()));
    if let Some(t) = trim {
        args.push(Arg::Word("-t"));
        args.push(Arg::Millis(t.length_ms));
    }
    args.push(Arg::Word("-map"));
    args.push(Arg::Word("0:v:0"));
    if has_audio {
        args.push(Arg::Word("-map"));
        args.push(Arg::Word("0:a:0?"));
    }
    // §6.3: never inherit variable timestamps.
    args.push(Arg::Word("-fps_mode"));
    args.push(Arg::Word("cfr"));
    args.push(Arg::Word("-r"));
    args.push(Arg::Fps {
        num: fps_num.max(1),
        den: fps_den.max(1),
    });
    if let Some(w) = resize_width {
        args.push(Arg::Word("-vf"));
        args.push(Arg::Filter(Filter::ScaleWidth(w)));
    }
    args.push(Arg::Word("-c:v"));
    args.push(Arg::Word(enc.vcodec));
    if let Some(kbps) = enc.vbitrate_kbps {
        // Quality (1..100) modulates the nominal bitrate linearly around the
        // measured 1500 kbps default, so the UI knob means something.
        let scaled = (kbps as u32 * quality.clamp(1, 100) as u32 / 90).clamp(200, 20_000);
        args.push(Arg::Word("-b:v"));
        args.push(Arg::Kbps(scaled));
    }
    for extra in enc.quality_args {
        args.push(Arg::Word(extra));
    }
    args.push(Arg::Word("-pix_fmt"));
    args.push(Arg::Word("yuv420p"));
    if has_audio {
        args.push(Arg::Word("-c:a"));
        args.push(Arg::Word(enc.acodec));
        args.push(Arg::Word("-b:a"));
        args.push(Arg::Kbps(enc.abitrate_kbps));
        args.push(Arg::Word("-ar"));
        args.push(Arg::Number(enc.audio_rate));
        args.push(Arg::Word("-sample_fmt"));
        args.push(Arg::Word(enc.audio_fmt));
    }
    for extra in enc.container {
        args.push(Arg::Word(extra));
    }
    if enc.movflags {
        args.push(Arg::Word("-movflags"));
        args.push(Arg::Word("+faststart"));
    }
    args.push(Arg::Path(PathRef::Output, PathOpts::default()));
    args
}

// ---------------------------------------------------------------------------
// GIF
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GifSource {
    /// A PNG the vips step wrote: the only way a still image becomes a GIF.
    Still,
}

/// `-i <temp png> -vf palette… -loop 0 <out>`
pub fn build_gif_from_png(_source: GifSource) -> Vec<Arg> {
    let mut args = base();
    args.push(Arg::Word("-i"));
    args.push(Arg::Path(PathRef::Temp(0), PathOpts::default()));
    args.push(Arg::Word("-vf"));
    args.push(Arg::Filter(Filter::PaletteFromStill));
    args.push(Arg::Word("-loop"));
    args.push(Arg::Number(0));
    args.push(Arg::Path(PathRef::Output, PathOpts::default()));
    args
}

pub fn build_gif_from_video(fps: u8, max_width: u32, trim: Option<Trim>) -> Vec<Arg> {
    let mut args = base();
    if let Some(t) = trim {
        args.push(Arg::Word("-ss"));
        args.push(Arg::Millis(t.start_ms));
    }
    args.push(Arg::Word("-i"));
    args.push(Arg::Path(PathRef::Input, PathOpts::default()));
    if let Some(t) = trim {
        args.push(Arg::Word("-t"));
        args.push(Arg::Millis(t.length_ms));
    }
    args.push(Arg::Word("-vf"));
    args.push(Arg::Filter(Filter::PaletteFromVideo { fps, max_width }));
    args.push(Arg::Word("-loop"));
    args.push(Arg::Number(0));
    args.push(Arg::Path(PathRef::Output, PathOpts::default()));
    args
}

/// Flags every invocation starts with: no stdin, overwrite our own temp/output,
/// a log level whose noise cannot mask a real error, and the closed protocol set
/// the input is allowed to reach.
fn base() -> Vec<Arg> {
    let mut args = vec![
        Arg::Word("-nostdin"),
        Arg::Word("-y"),
        Arg::Word("-hide_banner"),
        Arg::Word("-loglevel"),
        Arg::Word("error"),
    ];
    args.extend(protocol_guard());
    args
}

// ---------------------------------------------------------------------------
// ffprobe
// ---------------------------------------------------------------------------

/// `ffprobe -protocol_whitelist … -v error -select_streams a:0 -show_entries … <path>`
pub fn build_probe_audio() -> Vec<Arg> {
    let mut args: Vec<Arg> = protocol_guard().to_vec();
    args.extend([
        Arg::Word("-v"),
        Arg::Word("error"),
        Arg::Word("-select_streams"),
        Arg::Word("a:0"),
        Arg::Word("-show_entries"),
        Arg::Word("stream=codec_name,sample_rate,channels,sample_fmt,bits_per_raw_sample"),
        Arg::Word("-show_entries"),
        // Needed by the verifier: a probe without codec_type cannot tell an
        // audio output from an empty container.
        Arg::Word("stream=codec_type"),
        Arg::Word("-show_entries"),
        Arg::Word("format=duration"),
        Arg::Word("-of"),
        Arg::Word("default=nw=1"),
        Arg::Path(PathRef::Input, PathOpts::default()),
    ]);
    args
}

pub fn build_probe_video() -> Vec<Arg> {
    let mut args: Vec<Arg> = protocol_guard().to_vec();
    args.extend([
        Arg::Word("-v"),
        Arg::Word("error"),
        Arg::Word("-select_streams"),
        Arg::Word("v:0"),
        Arg::Word("-show_entries"),
        Arg::Word("stream=codec_name,width,height,r_frame_rate,avg_frame_rate,pix_fmt"),
        Arg::Word("-show_entries"),
        Arg::Word("stream=codec_type"),
        Arg::Word("-show_entries"),
        Arg::Word("format=duration"),
        Arg::Word("-of"),
        Arg::Word("default=nw=1"),
        Arg::Path(PathRef::Input, PathOpts::default()),
    ]);
    args
}

/// `ffprobe -protocol_whitelist … -v error -show_entries stream=codec_type
///  -show_entries format=duration -of default=nw=1 <path>`
///
/// The cheapest "which streams exist" probe. The video path needs it before it
/// can plan an audio rate, and it is the opening contact with the container, so it
/// carries the same guard as every other input.
pub fn build_probe_streams() -> Vec<Arg> {
    let mut args: Vec<Arg> = protocol_guard().to_vec();
    args.extend([
        Arg::Word("-v"),
        Arg::Word("error"),
        Arg::Word("-show_entries"),
        Arg::Word("stream=codec_type"),
        Arg::Word("-show_entries"),
        Arg::Word("format=duration"),
        Arg::Word("-of"),
        Arg::Word("default=nw=1"),
        Arg::Path(PathRef::Input, PathOpts::default()),
    ]);
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(args: &[Arg]) -> Vec<&'static str> {
        args.iter()
            .filter_map(|a| match a {
                Arg::Word(w) => Some(*w),
                _ => None,
            })
            .collect()
    }

    fn number_after(args: &[Arg], flag: &str) -> Option<u32> {
        let pos = args
            .iter()
            .position(|a| matches!(a, Arg::Word(w) if *w == flag))?;
        match args.get(pos + 1)? {
            Arg::Number(n) => Some(*n),
            Arg::Kbps(n) => Some(*n),
            _ => None,
        }
    }

    /// Measured trap: `-b:a 192` means 192 **bits**/s, not kbit/s. ffmpeg then
    /// prints "maybe you mean 192k" and crawls; the stall detector kills it.
    #[test]
    fn every_bitrate_carries_the_k_suffix() {
        for to in [
            TargetFormat::Mp3,
            TargetFormat::M4a,
            TargetFormat::Aac,
            TargetFormat::Opus,
        ] {
            let enc = AudioEncode::for_target(to, 44100, 16, 90).unwrap();
            let args = build_audio(&enc, false);
            if let Some(pos) = args.iter().position(|a| matches!(a, Arg::Word("-b:a"))) {
                assert!(
                    matches!(args[pos + 1], Arg::Kbps(_)),
                    "{to:?} passes a bare number to -b:a: {:?}",
                    args[pos + 1]
                );
            }
        }
        for to in [TargetFormat::Mp4, TargetFormat::Webm] {
            let enc = VideoEncode::for_target(to, 44100).unwrap();
            let args = build_video(&enc, 25, 1, true, None, None, 90);
            for flag in ["-b:v", "-b:a"] {
                if let Some(pos) = args.iter().position(|a| matches!(a, Arg::Word(w) if *w == flag)) {
                    // `-b:v 0` is the documented VP9 constant-quality form, so a
                    // bare zero is legal; any other bare number is the trap.
                    let value = &args[pos + 1];
                    assert!(
                        matches!(value, Arg::Kbps(_))
                            || *value == Arg::Number(0)
                            || *value == Arg::Word("0"),
                        "{to:?} passes a bare number to {flag}: {value:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn clamp_rate_never_invents_a_rate() {
        assert_eq!(clamp_rate(44100, MP3_RATES), 44100);
        assert_eq!(clamp_rate(96000, MP3_RATES), 48000);
        assert_eq!(clamp_rate(192000, MP3_RATES), 48000);
        assert_eq!(clamp_rate(48000, OPUS_RATES), 48000);
        assert_eq!(clamp_rate(44100, OPUS_RATES), 24000);
        assert_eq!(clamp_rate(44100, AAC_RATES), 44100);
        assert_eq!(clamp_rate(96000, AAC_RATES), 96000);
        // Below every supported rate: the smallest, never a negative/wrapped one.
        assert_eq!(clamp_rate(4000, MP3_RATES), 8000);
    }

    #[test]
    fn every_audio_target_declares_codec_rate_and_format() {
        for to in [
            TargetFormat::Mp3,
            TargetFormat::Wav,
            TargetFormat::Flac,
            TargetFormat::M4a,
            TargetFormat::Aac,
            TargetFormat::Ogg,
            TargetFormat::Opus,
        ] {
            let enc = AudioEncode::for_target(to, 44100, 16, 90).expect("encode");
            let args = build_audio(&enc, false);
            assert!(number_after(&args, "-ar").is_some(), "{to:?} has no -ar");
            assert!(
                number_after(&args, "-sample_fmt").is_none(),
                "-sample_fmt carries a word, not a number"
            );
            let w = words(&args);
            assert!(w.contains(&"-sample_fmt"), "{to:?}");
            assert!(w.contains(&"-vn"), "{to:?}");
            assert!(w.contains(&"0:a:0"), "{to:?}");
            assert!(w.contains(&"-map_metadata"), "{to:?}");
            assert!(w.contains(&"-1"), "{to:?}");
            assert!(args.contains(&Arg::Path(PathRef::Output, PathOpts::default())));
        }
    }

    #[test]
    fn opus_is_48k_and_m4a_is_ipod() {
        let opus = AudioEncode::for_target(TargetFormat::Opus, 44100, 16, 90).unwrap();
        assert_eq!(opus.rate, 48000);
        assert_eq!(opus.sample_fmt, "s16");
        let mp3 = AudioEncode::for_target(TargetFormat::Mp3, 44100, 16, 90).unwrap();
        assert_eq!(mp3.rate, 44100, "mp3 keeps a supported source rate");
        let m4a = AudioEncode::for_target(TargetFormat::M4a, 44100, 16, 90).unwrap();
        assert!(m4a.container.contains(&"ipod"));
        assert!(m4a.container.contains(&"+faststart"));
        let aac = AudioEncode::for_target(TargetFormat::Aac, 44100, 16, 90).unwrap();
        assert!(aac.container.contains(&"adts"));
    }

    #[test]
    fn wav_and_flac_keep_24_bit_when_the_source_is_wide() {
        let wav = AudioEncode::for_target(TargetFormat::Wav, 96000, 24, 90).unwrap();
        assert_eq!(wav.codec, "pcm_s24le");
        assert_eq!(wav.sample_fmt, "s32");
        assert_eq!(wav.rate, 96000);
        let flac = AudioEncode::for_target(TargetFormat::Flac, 96000, 24, 90).unwrap();
        assert_eq!(flac.sample_fmt, "s32");
        let narrow = AudioEncode::for_target(TargetFormat::Flac, 44100, 16, 90).unwrap();
        assert_eq!(narrow.sample_fmt, "s16");
    }

    #[test]
    fn audio_never_uses_a_gpl_or_unmeasured_encoder() {
        for to in [
            TargetFormat::Mp3,
            TargetFormat::Wav,
            TargetFormat::Flac,
            TargetFormat::M4a,
            TargetFormat::Aac,
            TargetFormat::Ogg,
            TargetFormat::Opus,
        ] {
            let enc = AudioEncode::for_target(to, 44100, 16, 90).unwrap();
            assert!(
                !enc.codec.contains("x264") && !enc.codec.contains("x265"),
                "{to:?} chose {}",
                enc.codec
            );
        }
    }

    #[test]
    fn video_is_cfr_and_never_uses_x264() {
        for to in [
            TargetFormat::Mp4,
            TargetFormat::Mkv,
            TargetFormat::Mov,
            TargetFormat::Webm,
        ] {
            let enc = VideoEncode::for_target(to, 44100).unwrap();
            assert_ne!(enc.vcodec, "libx264");
            let args = build_video(&enc, 30000, 1001, true, None, None, 90);
            let w = words(&args);
            assert!(w.contains(&"-fps_mode"), "{to:?}");
            assert!(w.contains(&"cfr"), "{to:?}");
            let pos = args
                .iter()
                .position(|a| *a == Arg::Word("-fps_mode"))
                .unwrap();
            assert_eq!(args[pos + 1], Arg::Word("cfr"));
            assert!(args.contains(&Arg::Fps {
                num: 30000,
                den: 1001
            }));
            assert!(w.contains(&"yuv420p"));
        }
    }

    #[test]
    fn webm_is_vp9_opus_and_the_rest_are_openh264_aac() {
        let webm = VideoEncode::for_target(TargetFormat::Webm, 44100).unwrap();
        assert_eq!(webm.vcodec, "libvpx-vp9");
        assert_eq!(webm.acodec, "libopus");
        assert_eq!(webm.audio_rate, 48000);
        let mp4 = VideoEncode::for_target(TargetFormat::Mp4, 44100).unwrap();
        assert_eq!(mp4.vcodec, "libopenh264");
        assert_eq!(mp4.acodec, "aac");
        assert!(mp4.movflags);
    }

    #[test]
    fn trim_seeks_before_the_input_and_sets_length_after() {
        let enc = VideoEncode::for_target(TargetFormat::Mp4, 44100).unwrap();
        let args = build_video(
            &enc,
            25,
            1,
            false,
            None,
            Some(Trim {
                start_ms: 500,
                length_ms: 1000,
            }),
            90,
        );
        let ss = args.iter().position(|a| *a == Arg::Word("-ss")).unwrap();
        let i = args.iter().position(|a| *a == Arg::Word("-i")).unwrap();
        let t = args.iter().position(|a| *a == Arg::Word("-t")).unwrap();
        assert!(ss < i && i < t);
        assert_eq!(args[ss + 1], Arg::Millis(500));
        assert_eq!(args[t + 1], Arg::Millis(1000));
    }

    #[test]
    fn audio_streams_are_optional_so_silent_video_still_converts() {
        let enc = VideoEncode::for_target(TargetFormat::Mp4, 44100).unwrap();
        let with = build_video(&enc, 25, 1, true, None, None, 90);
        assert!(words(&with).contains(&"0:a:0?"));
        let without = build_video(&enc, 25, 1, false, None, None, 90);
        assert!(!words(&without).contains(&"0:a:0?"));
        assert!(!words(&without).contains(&"-c:a"));
    }

    #[test]
    fn gif_from_a_still_reads_only_a_temp_png() {
        let args = build_gif_from_png(GifSource::Still);
        let i = args
            .iter()
            .position(|a| *a == Arg::Word("-i"))
            .expect("-i present");
        assert_eq!(args[i + 1], Arg::Path(PathRef::Temp(0), PathOpts::default()));
        assert!(!args
            .iter()
            .any(|a| matches!(a, Arg::Path(PathRef::Input, _))));
        assert_eq!(args[i + 2], Arg::Word("-vf"));
    }

    /// The protocol set is pinned: `file` for the admitted path, and the two
    /// pseudo-protocols ffmpeg itself defaults to for a file input. Anything
    /// that could fetch (http/tcp/udp/rtmp) or splice (`concat`, `subfile`,
    /// `pipe`) must never appear here.
    #[test]
    fn input_protocol_set_is_closed_and_pinned() {
        assert_eq!(INPUT_PROTOCOLS, "file,crypto,data");
        for banned in [
            "http", "https", "tcp", "udp", "rtmp", "rtsp", "ftp", "concat", "subfile", "pipe",
            "crypto+file", "*", "all",
        ] {
            assert!(
                !INPUT_PROTOCOLS.split(',').any(|p| p == banned),
                "{banned} must not be whitelisted"
            );
        }
    }

    /// Every builder that names an input carries the guard, exactly once, and
    /// ahead of the input path slot — so it applies to that input. A builder that
    /// forgets it would let a container-driven nested open reach any protocol.
    #[test]
    fn every_ffmpeg_and_ffprobe_input_carries_the_protocol_guard() {
        let mut built: Vec<(String, Vec<Arg>)> = Vec::new();
        for to in [
            TargetFormat::Mp3,
            TargetFormat::Wav,
            TargetFormat::Flac,
            TargetFormat::M4a,
            TargetFormat::Aac,
            TargetFormat::Ogg,
            TargetFormat::Opus,
        ] {
            let enc = AudioEncode::for_target(to, 44100, 16, 90).unwrap();
            built.push((format!("audio {to:?}"), build_audio(&enc, false)));
        }
        for to in [
            TargetFormat::Mp4,
            TargetFormat::Mkv,
            TargetFormat::Mov,
            TargetFormat::Webm,
        ] {
            let enc = VideoEncode::for_target(to, 44100).unwrap();
            built.push((
                format!("video {to:?}"),
                build_video(&enc, 25, 1, true, Some(640), None, 90),
            ));
        }
        built.push(("gif from png".into(), build_gif_from_png(GifSource::Still)));
        built.push(("gif from video".into(), build_gif_from_video(12, 480, None)));
        built.push(("probe audio".into(), build_probe_audio()));
        built.push(("probe video".into(), build_probe_video()));
        built.push(("probe streams".into(), build_probe_streams()));

        for (label, args) in built {
            let hits: Vec<usize> = args
                .iter()
                .enumerate()
                .filter(|(_, a)| **a == Arg::Word("-protocol_whitelist"))
                .map(|(i, _)| i)
                .collect();
            assert_eq!(hits.len(), 1, "{label}: guard count {:?}", hits);
            let at = hits[0];
            assert_eq!(
                args[at + 1],
                Arg::Word(INPUT_PROTOCOLS),
                "{label}: guard value must be the closed static word"
            );
            // The guard is an input option: it must precede the path it governs.
            let first_path = args
                .iter()
                .position(|a| matches!(a, Arg::Path(_, _)))
                .expect("a path slot");
            assert!(at < first_path, "{label}: guard after the input path");
            if label.starts_with("audio") || label.starts_with("video") || label == "gif from video" {
                let i = args
                    .iter()
                    .position(|a| *a == Arg::Word("-i"))
                    .expect("-i");
                assert!(at < i, "{label}: guard must precede -i");
            }
        }
    }

    /// The guard is static text: no filename, number or filter can extend it.
    #[test]
    fn protocol_guard_is_words_only() {
        let args = protocol_guard();
        assert!(matches!(args[0], Arg::Word("-protocol_whitelist")));
        assert!(matches!(args[1], Arg::Word(_)));
        assert!(args.iter().all(|a| !a.is_option() || *a == Arg::Word("-protocol_whitelist")));
    }

    #[test]
    fn video_to_gif_filter_caps_rate_and_width() {
        let f = Filter::PaletteFromVideo {
            fps: 12,
            max_width: 480,
        }
        .render();
        assert!(f.starts_with("fps=12,scale=min(480\\,iw):-1:flags=lanczos"));
        assert!(f.contains("palettegen=stats_mode=diff"));
        assert!(f.contains("paletteuse=dither=bayer:bayer_scale=3"));
    }

    #[test]
    fn render_only_emits_filter_grammar_characters() {
        for f in [
            Filter::PaletteFromStill,
            Filter::PaletteFromVideo {
                fps: 30,
                max_width: 1024,
            },
            Filter::ScaleWidth(640),
        ] {
            let s = f.render();
            assert!(
                s.chars().all(|c| c.is_ascii() && !c.is_control()),
                "filter has non-ascii: {s}"
            );
            // A palette graph has exactly two chain separators; anything more
            // means we accidentally composed two graphs.
            assert!(
                s.matches(';').count() <= 2,
                "unexpected graph shape: {s}"
            );
            assert!(!s.contains('"') && !s.contains('\''), "quotes in {s}");
        }
    }

    #[test]
    fn quality_scales_the_video_bitrate_but_stays_bounded() {
        let enc = VideoEncode::for_target(TargetFormat::Mp4, 44100).unwrap();
        let low = build_video(&enc, 25, 1, true, None, None, 10);
        let high = build_video(&enc, 25, 1, true, None, None, 100);
        assert!(number_after(&low, "-b:v").unwrap() < number_after(&high, "-b:v").unwrap());
        assert!(number_after(&low, "-b:v").unwrap() >= 200);
        assert!(number_after(&high, "-b:v").unwrap() <= 20_000);
    }
}
