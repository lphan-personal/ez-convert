# P4 — Windows engine pack, built on a GitHub Windows runner

Card `t_1b6f36fe`. Date: 2026-11-02. Final run: **41437444732** (branch
`winpack/t_1b6f36fe`, sha `cb8446df`), workflow `windows-engine-pack`, job
`pack (windows-engine-pack-x86_64.zip)`, 5 min 39 s.

## What was wrong

`ez-convert-Windows-unsigned` is the app shell only (2.1 MB): macOS has an engine pack
(`macos-engine-pack-arm64`, `-x86_64`), Windows had no artifact at all, so the installer
produced a program that could not convert anything. The Windows *test* host `win11-dev`
cannot fetch the engine sources at all (no outbound TCP/DNS), so the pack has to be built
where the network is: a GitHub-hosted Windows runner.

## What ships

`windows-engine-pack-x86_64.zip`

| property | value |
|---|---|
| bytes | 460,687,860 (439.3 MiB) — limit is 1 GiB |
| sha256 | `521641666b1adb41c93df99926b14644b61406ab6e47012fd71bff89f6987180` |
| files in `MANIFEST.sha256` | 8,378 (8,372 shipped engine files + notices/README) |
| workflow artifact | `windows-engine-pack` (run 41437444732), with `.sha256` and `build-report.txt` |

Layout — the names `EnginePaths::from_root` (`src-tauri/src/engine/proc.rs`) resolves on
Windows, i.e. what `adopt_pack()` accepts:

```
engines/vips/      43 files   libvips 8.18.7 spec-aom, DLL closure, licenses/
engines/ffmpeg/     2 files   ffmpeg.exe + ffprobe.exe (BtbN LGPL, enable-gpl=0)
engines/soffice/  8,327 files LibreOffice 26.8.2.2, installed tree, stripped
licenses/           7 files   notices, configure output, MSI digest, strip manifest, gate output
MANIFEST.sha256               every shipped file, hashed
README.txt                    what it is / how to install
```

## Pinned inputs (all digest-checked before unpacking)

| input | digest |
|---|---|
| libvips 8.18.7 "spec-aom" dist tarball (in-repo, `packaging/windows/inputs/`) | `0dff47413690bf9ae3d0599e5fb4a04ff03419dcb191af80611fbcb73b2b9dc9` |
| BtbN `ffmpeg-N-128273-g836436cfb9-win64-lgpl.zip` (dated tag `autobuild-2026-11-01-12-49`) | zip `00bfdb3e…`; `ffmpeg.exe` `da90280e…`; `ffprobe.exe` `894f68a5…` (the two exe digests are the inventory rows) |
| LibreOffice `26.8.2 Win x86-64` MSI (TDF) | `ff4b911403b67ee9e6249f820f9782c44bb32c94fdaa45b0da53a8e419e5307a` |

## Gates, and what each one said on the final run

| gate | result |
|---|---|
| vips relocated run + config | `vips-8.18.7` runs from the pack; `image quantisation … false`, `… with libheif: true` |
| ffmpeg build | `-buildconf` has no `--enable-gpl`/`--enable-nonfree` |
| LibreOffice | `soffice.com --version` → `LibreOffice 26.8.2.2 9dea406ab7695df6325b11b0e19fdca8db4b7276` |
| strip (`t_20fae747`) | `culmus 0`, `gallery 175`, `help 5,334` removed; 13,805 → 8,327 files, 709,176,098 B; survivors asserted absent |
| dependency closure | `37 PE objects checked, 0 unresolved imports` (allow-listed Windows system DLLs listed in the log) |
| banned-import audit | PASS — no imagequant/poppler/magick/x265/x264/jbig |
| licence gate | inventory `119 rows, 0 failures, 0 warnings`; scans `vips 43/0`, `ffmpeg 2/0`, `soffice 8,296/0`, pack-root projection `3/0` |
| smoke, judged by the product | see below |
| artifact verify | zip unpacked elsewhere and all 8,378 MANIFEST digests re-checked: `artifact verified` |

Smoke, 9 s, every product inspected rather than an exit code trusted:

```
image   png -> jpg 1989 B, 64x48 (source 64x48)
image   heic -> jpg 3444 B (libheif decode — the stock builds fail this)
audio   wav -> mp3 2494 B, codec=mp3 (libmp3lame)
video   mp4 -> webm 2659 B, codec=vp9 (libvpx-vp9 + libopus, the app's own route)
document docx -> pdf 39984 B, 1 page object(s); docx -> txt 349 B with MARKER-ALPHA + café
FAMILY image PASS / audio PASS / video PASS / document PASS
```

## Decisions taken while getting here (each measured, not assumed)

1. **The pack is the LibreOffice *installed* tree, not `msiexec /a` output.** The
   administrative install lays out 19,418 files / 1.60 GB — every file the MSI can place
   anywhere, including `Fonts/` (carries the GPL-2.0 Culmus families the licence decision
   strips) and `System{,64}/`. The installed tree is 13,805 files / 733,833,618 B, which is
   the tree P2 shipped and the gate was made green against.
2. **vips ships from a pinned in-repo tarball.** There is no public GPL-free Windows
   libvips build; the S2 dist is the one measured and audited, and the inventory pins all
   43 per-file digests, so a rebuild from a newer MSYS2 would *change* shipped bytes. The
   tarball's digest is pinned in the script and every file is re-hashed by the gate.
3. **`zip -1`.** 460 MB zipped in 3 s; the acceptance criterion is "a zip under 1 GiB",
   and compression level is not what makes the artifact trustworthy — the manifest is.
4. **The licence gate was rewritten for speed, not for verdicts.** `--scan` re-read all
   119 inventory rows per file and forked grep/sed per file: 2 min 06 s for the 8,327-file
   tree on Linux. It now parses the inventory once, filters rows by their literal prefix
   before any glob, and reuses the previous verdict for consecutive files in the same
   group: 12 s, same attributions, selftest 13/13. On the runner the soffice scan is 33 s.

## Failures hit (all distinct, all in the job log; none was a blind retry)

| run | failure | fix |
|---|---|---|
| #1 | `/a` extraction put the GPL-2.0 Culmus fonts under `Fonts/`; the strip assertion caught them | drop the OS font payload; then #3 replaced `/a` with the installed tree |
| #2 | closure flagged 37 objects for importing `KERNEL32.dll` etc. | allow-list compared full names against suffixes; strip `.dll`, add avrt/d2d1/usp10/avicap32/normaliz/bcryptprimitives |
| #3 | licence gate: `122 rows, 3 failures` | CRLF checkout (`.gitattributes` lacked `*.tsv`): strip CR in both loops, pin `*.tsv` to LF, selftest case added |
| #4, #5 | cancelled mid-run by a push to the same branch (concurrency) | run one at a time |
| #6 | gate was the wall (multi-minute per tree) | the `--scan` rewrite above |
| #7 | `ffmpeg` audio call had no `-i`, so it took the fixture as its output | add `-i` (product code was never implicated — `build_audio()` pushes `-i`) |
| #8 | `zip` got a `cygpath -w` path | MSYS2 binaries take POSIX paths; `win()` is for native Windows tools only |
| #9 | `zip` still failed: `--out dist` is relative and the zip runs inside `cd "$PACK"` | absolutise `--out` at parse time |
| #10 | manifest listed 2 dotfiles the zip excludes (8,380 vs 8,378) | exclude dot-named files from the manifest with `! -name '.*'` |

## Not done here

- The **installer** still has to consume this pack (v1.0.0 shipped macOS-only; a Windows
  v1.0.1 with engines is a stakeholder decision). `stage-engines.sh --vips --ffmpeg
  --lo-pack` takes the three trees from the unpacked pack unchanged.
- The pack is unsigned (`docs/DISTRIBUTION.md`): SmartScreen will warn on first run.
- `pdfium`/`ezc-pdf`, `resvg`, `pandoc` remain unshipped, as documented.
