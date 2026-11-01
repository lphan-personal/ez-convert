//! `engine/vips.rs` — the libvips argv builder (images, `ARCHITECTURE.md` §6.1).
//!
//! Pure: returns argv data, never spawns. The measured facts behind each choice
//! live in `evidence/c2-media.md` §2 and `§6.1`:
//!
//! - `vips <op> '<in>[fail-on=error]' '<out>[Q=90,strip]'` — bracket filename
//!   options are the CLI's documented option syntax. Measured: the loader flag
//!   makes a truncated JPEG fail with exit 1, the saver `Q` changes output size
//!   (3300 B at Q80 vs 5320 B at Q90) and `strip` removes EXIF on
//!   jpg/png/webp/tiff.
//! - `--fail-on` is **not** accepted by `copy` and `--Q`/`--strip` are **not**
//!   accepted by a loader op (`Unknown option`) — the bracket form is the only
//!   spelling that carries both at once in one process.
//! - `autorot` bakes the rotation and rewrites `Orientation` to 1 (400x200 →
//!   200x400). Plain `copy` does not rotate, which is why a conversion that
//!   drops the EXIF block (PNG) must use `autorot` instead.
//! - AVIF output keeps the exact argv measured on the shipping dist
//!   (`heifsave --compression av1 --Q 60 --effort 6 --bitdepth 8`) plus one
//!   correction measured there afterwards: `heifsave` takes **flags**, not a
//!   bracket, and `--keep all` overrides `--strip`, so the saver slot is a bare
//!   path and strip is `--strip` alone (keep is `--keep all` alone).

use super::plan::{Arg, Compression, PathOpts, PathRef};
use crate::model::types::{CanonicalType, TargetFormat};

/// Which vips operation runs. A closed enum because `argv[1]` must be a literal
/// (`§6.0.1` A2): a user string in that slot is an arbitrary-command primitive
/// (`vips system "cmd.exe /c …"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Copy,
    Autorot,
    Resize,
    HeifSave,
}

impl Op {
    pub fn as_str(self) -> &'static str {
        match self {
            Op::Copy => "copy",
            Op::Autorot => "autorot",
            Op::Resize => "resize",
            Op::HeifSave => "heifsave",
        }
    }
}

/// Sources that can carry an EXIF orientation tag. `autorot` is a measured
/// no-op on HEIC (libheif applies `irot` itself) and harmless on the others.
fn carries_orientation(from: CanonicalType) -> bool {
    matches!(
        from,
        CanonicalType::Jpeg | CanonicalType::Tiff | CanonicalType::Heic | CanonicalType::Avif
    )
}

/// Saver quality for the target, `None` where the format has no quality knob.
pub fn quality_for(to: TargetFormat, quality: u8) -> Option<u8> {
    match to {
        TargetFormat::Jpg | TargetFormat::Webp => Some(quality.clamp(1, 100)),
        // AVIF quality is passed as a `--Q` flag on heifsave (see build_still).
        TargetFormat::Avif => None,
        _ => None,
    }
}

/// Saver compression for the target, `None` where the default is right.
pub fn compression_for(to: TargetFormat) -> Option<Compression> {
    match to {
        TargetFormat::Png => Some(Compression::Png(9)),
        TargetFormat::Tiff => Some(Compression::Deflate),
        _ => None,
    }
}

/// Why this plan uses the op it uses — shown in the job log.
pub fn notes_for(from: CanonicalType) -> Vec<&'static str> {
    if carries_orientation(from) {
        vec![
            "orientation is baked by autorot (measured: 400x200 -> 200x400, tag rewritten to 1)",
            "EXIF/ICC are kept by default; `strip` removes them (measured)",
        ]
    } else {
        vec![
            "no EXIF orientation on this family: copy keeps bytes and metadata as-is",
            "EXIF/ICC are kept by default; `strip` removes them (measured)",
        ]
    }
}

/// Build the single vips invocation for a still-image conversion.
///
/// `out` is a [`PathRef`] so the same builder serves a direct conversion
/// (`PathRef::Output`) and the normalise step of GIF output (`PathRef::Temp`).
pub fn build_still(
    from: CanonicalType,
    to: TargetFormat,
    out: PathRef,
    input_opts: PathOpts,
    saver_opts: PathOpts,
    resize_percent: Option<u8>,
) -> (&'static str, Vec<Arg>) {
    if to == TargetFormat::Avif {
        // The argv measured on the shipping dist (S2b/S4: 34 assertions, 0
        // failures), with one correction measured on that same dist (card
        // t_ade3f5a7): `heifsave` takes its options as **flags**. The CLI's
        // bracket syntax is parsed only on arguments that become a
        // `VipsImage`, so a `[strip]` suffix on this filename is a literal
        // filename — `heifsave in.jpg "out.avif[strip]"` exits 0 and the
        // requested path never appears. The saver slot is therefore bare.
        // `--bitdepth 8` stops libheif from writing 12-bit output.
        let strip = saver_opts.strip;
        let mut args = vec![
            Arg::Word(Op::HeifSave.as_str()),
            Arg::Path(PathRef::Input, input_opts),
            // No bracket: heifsave would take it as part of the file name.
            Arg::Path(out, PathOpts::default()),
            Arg::Word("--compression"),
            Arg::Word("av1"),
            Arg::Word("--Q"),
            Arg::Number(60),
            Arg::Word("--effort"),
            Arg::Number(6),
            Arg::Word("--bitdepth"),
            Arg::Number(8),
        ];
        if strip {
            // `--keep all` is the default, but it also **overrides** `--strip`
            // (measured on `fixtures/exif_icc.jpg`: `--keep all --strip` = 1740 B
            // with EXIF present, `--strip` alone = 760 B with none), so the two
            // flags are mutually exclusive. Strip is `--strip` alone.
            args.push(Arg::Word("--strip"));
        } else {
            args.push(Arg::Word("--keep"));
            args.push(Arg::Word("all"));
        }
        return ("vips heifsave (av1)".into(), args);
    }

    match resize_percent {
        Some(p) if p != 100 => (
            "vips resize",
            vec![
                Arg::Word(Op::Resize.as_str()),
                Arg::Path(PathRef::Input, input_opts),
                Arg::Path(out, saver_opts),
                Arg::Percent(p),
            ],
        ),
        _ if carries_orientation(from) => (
            "vips autorot",
            vec![
                Arg::Word(Op::Autorot.as_str()),
                Arg::Path(PathRef::Input, input_opts),
                Arg::Path(out, saver_opts),
            ],
        ),
        _ => (
            "vips copy",
            vec![
                Arg::Word(Op::Copy.as_str()),
                Arg::Path(PathRef::Input, input_opts),
                Arg::Path(out, saver_opts),
            ],
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guard() -> PathOpts {
        PathOpts {
            fail_on: true,
            ..PathOpts::default()
        }
    }

    #[test]
    fn jpeg_to_png_uses_autorot_with_loader_guard() {
        let (purpose, args) = build_still(
            CanonicalType::Jpeg,
            TargetFormat::Png,
            PathRef::Output,
            guard(),
            PathOpts {
                compression: Some(Compression::Png(9)),
                ..PathOpts::default()
            },
            None,
        );
        assert_eq!(purpose, "vips autorot");
        assert_eq!(args[0], Arg::Word("autorot"));
        assert_eq!(
            args[1],
            Arg::Path(
                PathRef::Input,
                PathOpts {
                    fail_on: true,
                    ..PathOpts::default()
                }
            )
        );
    }

    #[test]
    fn svg_and_png_go_through_copy_not_autorot() {
        for from in [CanonicalType::Png, CanonicalType::WebP, CanonicalType::Svg] {
            let (_, args) = build_still(from, TargetFormat::Jpg, PathRef::Output, guard(), PathOpts::default(), None);
            assert_eq!(args[0], Arg::Word("copy"), "{from:?}");
        }
    }

    #[test]
    fn tiff_keeps_its_loader_guard() {
        let (_, args) = build_still(
            CanonicalType::Tiff,
            TargetFormat::Jpg,
            PathRef::Output,
            guard(),
            PathOpts::default(),
            None,
        );
        assert!(matches!(
            &args[1],
            Arg::Path(PathRef::Input, o) if o.fail_on
        ));
        assert_eq!(args[0], Arg::Word("autorot"));
    }

    #[test]
    fn quality_and_compression_follow_the_target() {
        assert_eq!(quality_for(TargetFormat::Jpg, 90), Some(90));
        assert_eq!(quality_for(TargetFormat::Webp, 90), Some(90));
        assert_eq!(quality_for(TargetFormat::Png, 90), None);
        assert_eq!(quality_for(TargetFormat::Tiff, 90), None);
        assert_eq!(
            compression_for(TargetFormat::Tiff),
            Some(Compression::Deflate)
        );
        assert_eq!(compression_for(TargetFormat::Jpg), None);
    }

    #[test]
    fn resize_uses_a_percent_argument() {
        let (purpose, args) = build_still(
            CanonicalType::Jpeg,
            TargetFormat::Webp,
            PathRef::Output,
            guard(),
            PathOpts::default(),
            Some(50),
        );
        assert_eq!(purpose, "vips resize");
        assert_eq!(args[0], Arg::Word("resize"));
        assert_eq!(args[3], Arg::Percent(50));
    }

    #[test]
    fn avif_output_carries_the_measured_flag_set() {
        let (purpose, args) = build_still(
            CanonicalType::Jpeg,
            TargetFormat::Avif,
            PathRef::Output,
            guard(),
            PathOpts::default(),
            None,
        );
        assert!(purpose.contains("heifsave"));
        assert_eq!(args[0], Arg::Word("heifsave"));
        for needle in [
            Arg::Word("--compression"),
            Arg::Word("av1"),
            Arg::Word("--effort"),
            Arg::Number(6),
            Arg::Word("--bitdepth"),
            Arg::Number(8),
            Arg::Word("--keep"),
            Arg::Word("all"),
        ] {
            assert!(args.contains(&needle), "missing {needle:?} in {args:?}");
        }
        // No bracket saver options: heifsave takes flags, and an empty bracket
        // would render as "[]".
        assert!(matches!(args[2], Arg::Path(PathRef::Output, o) if !o.fail_on));
    }

    /// Card t_ade3f5a7 — AVIF + strip, pinned at the argv level.
    ///
    /// Two measured defects, one test:
    /// - the CLI parses a `[...]` group only on arguments that become a
    ///   `VipsImage`. `heifsave`'s filename is a plain string, so a `[strip]`
    ///   suffix is a **literal filename** — the run exits 0 and the requested
    ///   path never appears;
    /// - `--keep all` (the default, passed for determinism) **overrides**
    ///   `--strip`, so even a correctly-named file keeps the source EXIF.
    #[test]
    fn avif_strip_is_flags_only_and_never_brackets_the_saver_path() {
        let (_, strip_args) = build_still(
            CanonicalType::Jpeg,
            TargetFormat::Avif,
            PathRef::Output,
            guard(),
            PathOpts {
                strip: true,
                ..PathOpts::default()
            },
            None,
        );
        assert!(
            strip_args.contains(&Arg::Word("--strip")),
            "strip must be the `--strip` flag: {strip_args:?}"
        );
        assert!(
            !strip_args
                .windows(2)
                .any(|w| w == [Arg::Word("--keep"), Arg::Word("all")]),
            "`--keep all` cancels `--strip` (measured): {strip_args:?}"
        );
        match &strip_args[2] {
            Arg::Path(PathRef::Output, o) => assert_eq!(
                *o,
                PathOpts::default(),
                "the heifsave output slot must carry no bracket options"
            ),
            other => panic!("args[2] is not the output path: {other:?}"),
        }

        // The keep path is unchanged: still the measured `--keep all`, no strip.
        let (_, keep_args) = build_still(
            CanonicalType::Jpeg,
            TargetFormat::Avif,
            PathRef::Output,
            guard(),
            PathOpts::default(),
            None,
        );
        assert!(keep_args
            .windows(2)
            .any(|w| w == [Arg::Word("--keep"), Arg::Word("all")]));
        assert!(!keep_args.contains(&Arg::Word("--strip")));
        assert!(matches!(keep_args[2], Arg::Path(PathRef::Output, o) if o == PathOpts::default()));
    }

    #[test]
    fn first_argument_is_always_a_closed_enum_literal() {
        for from in [
            CanonicalType::Jpeg,
            CanonicalType::Png,
            CanonicalType::Tiff,
            CanonicalType::Avif,
            CanonicalType::Svg,
        ] {
            for to in [
                TargetFormat::Jpg,
                TargetFormat::Png,
                TargetFormat::Webp,
                TargetFormat::Tiff,
                TargetFormat::Avif,
            ] {
                let (_, args) = build_still(from, to, PathRef::Output, guard(), PathOpts::default(), None);
                match &args[0] {
                    Arg::Word(w) => assert!(
                        ["copy", "autorot", "resize", "heifsave"].contains(w),
                        "unexpected op {w}"
                    ),
                    other => panic!("argv[1] is not a literal: {other:?}"),
                }
            }
        }
    }
}
