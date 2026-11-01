# Card `t_ade3f5a7` — AVIF + strip: the saver bracket and the `--keep all` conflict

Status: **fixed and measured green on the shipping dist** (`win11-dev`, vips 8.18.7
`spec-aom`). Found by card `t_4b6ee179` (the Q3 suite on Windows), which left the four
`→ avif` rows red and filed this card. `model::matrix` was **not** touched here: every
`→ avif` cell keeps the value it had; the flip decision belongs to the matrix owner.

## The defect, in one paragraph

`engine::vips::build_still` rendered the saver's `PathOpts` bracket onto the output file
name for `TargetFormat::Avif` and passed `--keep all` unconditionally. Two independent
things were wrong with that, and either one alone breaks SPEC §5:

1. **The bracket is a literal file name on `heifsave`.** The CLI parses `[...]` only on
   arguments that become a `VipsImage` (`copy`, `autorot`, `resize`, `thumbnail`).
   `heifsave`'s output argument is a plain string, so `out.avif[strip]` is what gets
   written: exit **0**, no stderr, and nothing at the path the job asked for — the
   verify hop then reports the output as missing.
2. **`--keep all` overrides `--strip`.** So even with the path fixed, the "strip" run
   would carry the source EXIF/ICC and the metadata policy would still be red.

## What changed

| file | change |
|---|---|
| `src-tauri/src/engine/vips.rs` | `build_still` for `TargetFormat::Avif`: the saver slot is `Arg::Path(out, PathOpts::default())` (bare, no bracket), `--keep all` is emitted **only** when not stripping, `--strip` **only** when stripping. The rest of the measured flag set (`--compression av1 --Q 60 --effort 6 --bitdepth 8`) is unchanged, and so is the keep path's bytes. |
| `src-tauri/src/engine/vips.rs` (tests) | `avif_strip_is_flags_only_and_never_brackets_the_saver_path` — argv level: `--strip` present, `--keep all` absent, `args[2]` is `Path(Output, PathOpts::default())`; the keep path still sends `--keep all` and no `--strip`. |
| `src-tauri/src/engine/plan.rs` (tests) | `avif_strip_renders_a_bare_saver_path_and_no_keep_all` — plan level: the **rendered, real-path argv** for `jpeg → avif` with `strip_metadata = true` has no `--keep all`, has `--strip`, and its saver slot ends in `out.avif` with no `[`/`]`. This is the assertion that would have caught the bug the shipped suite hit. |
| `docs/ARCHITECTURE.md` | `§6.1` argv table gains **V8d** (the measurement), and the AVIF bullet now says the saver path is bare, strip is the flag, and `--keep all` is not inert — it beats `--strip`. The old text ("`--keep all` is the default, so metadata survival does not depend on passing it") was the assumption that hid this. |
| `evidence/q3-e2e.md` | the AVIF gap bullet now records the resolution instead of "not green on any dist yet". |

Nothing else moved: no engine set change, no dependency, no new license, no matrix cell.

## Proof 1 — RED, then GREEN, in the engine harness

`scripts/test-engine.sh` compiles the shipped module tree with plain `rustc` (no GTK, no
cargo). Both new tests were written **before** the fix and fail on the pre-fix builder
with the exact argv the card reports. The RED run below is taken at the head this lands on
(the Avif arm temporarily restored to its pre-fix shape, the tests untouched), so the
detection is demonstrated against the tree that ships, not against a paraphrase of it:

```
red-pre-fix.txt   test result: FAILED. 0 passed; 2 failed   EXIT=101
  `--keep all` cancels `--strip` (measured): ["heifsave", "…/in.jpg[fail-on=error,strip]",
  "…/out.avif[strip]", "--compression", "av1", "--Q", "60", "--effort", "6", "--bitdepth",
  "8", "--keep", "all", "--strip"]

green-post-fix.txt  test result: ok. 2 passed; 0 failed        EXIT=0
```

Full engine harness after the fix: **195 passed, 0 failed** (the base's 187, the 2 new
tests, and 6 that the `hebrew` card added to the same harness in between).

## Proof 2 — the four rows, on the Windows leg, before and after

Same suite (`scripts/e2e.sh`), same dist, same fixtures. The only difference is the tree:
`C:\ezc\repo2` (the `cb05606`-era checkout the parent card measured) vs `C:\ezc\repo3`
(this branch — 271 files, every `sha256` equal to the working tree on the Linux side).

Before (`evidence/q3-e2e/windows.txt`, card `t_4b6ee179`):

```
FAIL  image  jpeg->avif  … strip run failed: output failed verification: C:/ezc/q3work\out\meta-jpeg-to-avif-strip.avif is missing
FAIL  image  png->avif   … meta-png-to-avif-strip.avif is missing
FAIL  image  webp->avif  … meta-webp-to-avif-strip.avif is missing
FAIL  image  tiff->avif  … meta-tiff-to-avif-strip.avif is missing
SUMMARY RED  (4 direction failure(s), 0 control(s) not caught)
advertised=154 pass=150 fail=4 skip=0 controls=13/13 caught
```

After (`windows-full.txt`, `windows-media-only.txt`):

```
PASS  image     jpeg->avif                  meta=icc=kept strip=clean         899 B    234 ms  64x48 mean=124.9 899B
PASS  image     png->avif                   meta=icc=kept strip=clean         875 B    235 ms  64x48 mean=125.0 875B
PASS  image     webp->avif                  meta=icc=kept strip=clean         971 B    235 ms  64x48 mean=125.4 971B
PASS  image     tiff->avif                  meta=icc=kept strip=clean         875 B    233 ms  64x48 mean=125.0 875B

family     advertised   pass   fail   skip        (full run: media + documents)
image              37     37      0      0
audio              49     49      0      0
video              20     20      0      0
document           42     42      0      0

advertised=154 pass=154 fail=0 skip=0 controls=13/13 caught
SUMMARY GREEN  every advertised direction passed on this dist
```

The media-only run (`windows-media-only.txt`) is the narrower check: `advertised=112
pass=112 fail=0 skip=0 controls=13/13 caught`, `SUMMARY GREEN`. `strip=clean` is the
suite's own strip verdict (`e2e/q3/suite.rs`: ICC absent **and** no
`exif-ifd0-Orientation` in the `vipsheader -a` dump), not a claim taken from this document.
The gif/heic `→ avif` rows carry no metadata expectation on this dist (no fixture that
carries EXIF/ICC can be produced there) and were already PASS.

## Proof 3 — the mechanism, measured outside the suite

`probe-engine-native.txt`, raw `vips` + `vipsheader -a` on the shipping dist, native paths
(the form the card measured):

| call on `fixtures/exif_icc.jpg` | result |
|---|---|
| `heifsave in.jpg "C:/…/bracket.avif[strip]" … --keep all --strip` | exit **0**; **no file at `bracket.avif`**; a 1740 B file named `bracket.avif[strip]` |
| `heifsave in.jpg flags.avif … --strip` (what the fix emits) | exit 0; **760 B**; `exif-data` absent, `icc-profile-data` absent |
| `heifsave in.jpg keepall-strip.avif … --keep all --strip` | exit 0; 1740 B; EXIF **present**, ICC **present** |
| `heifsave in.jpg keep.avif … --keep all` (the keep path, unchanged) | exit 0; 1740 B; EXIF and ICC present |

`probe-suite-workdir.txt` reads the same facts back off the suite's own work dir: the four
strip outputs are 760 / 735 / 586 / 735 B against 1740 / 1715 / 1566 / 1739 B for their keep
twins, EXIF and ICC counts are `0` in every strip file and `1` in every keep file, and
`find /c/ezc/q3work3 -name '*[*'` returns **nothing** — no bracketed residue anywhere in
the destination tree.

One honesty note on the engine probe: with an **MSYS-style** path
(`/c/ezc/…/x.avif[strip]`) the same call fails differently — `unable to open for write`,
exit 1 — because that path is not a valid Windows path at all. Both outcomes are the same
defect (the requested path is never produced) but they are not the same transcript, so both
are kept: the native-path run is the card's measurement, and the MSYS one is recorded here
so nobody re-reads the difference as a fix.

## Environment for the Windows leg

| item | value |
|---|---|
| host | `win11-dev` (pve-ryo VM 110), Windows 11 Pro 10.0.26200.6584, 6 vCPU / 8 GB |
| vips | 8.18.7, `spec-aom` dist at `C:\ezc\dist` — the 43-file dist the app ships |
| ffmpeg | `N-128273-g836436cfb9-20261101`, BtbN `win64-lgpl`, `C:\ezc\ffmpeg` |
| LibreOffice | 26.8.2.2, `C:\Program Files\LibreOffice\program` |
| tree | `C:\msys64\ezc\repo3` (junction `C:\ezc\repo3`), 271 files, byte-identical to the branch working tree on the Linux side (every `sha256` equal); measured at this commit, i.e. rebased onto `4339541` (`p3-mac: …`), so the run is at the head it lands on |
| driver | `/c/msys64/ezc/q3avif.sh` — a copy of the parent card's `q3win.sh` with the checkout, work and report dirs changed (`C:/ezc/q3work3`, `C:/ezc/q3report3`); it only exports `EZC_VIPS_DIR`, `EZC_FFMPEG_DIR`, `EZC_SOFFICE_DIR`, `RUSTFLAGS` and `cd`s. No repo file is edited by the leg. |

Commands:

```bash
bash /c/msys64/ezc/q3avif.sh --media-only   # 112 advertised, all PASS
bash /c/msys64/ezc/q3avif.sh                # media + documents: 154 advertised, all PASS
```

## What this does **not** prove

- **The Linux leg.** This container has no `vips` and no AV1 encoder, so `→ avif` stays
  SKIP there; the CI runner's `libvips-tools` leg is unchanged and still skipped by design.
  The green evidence is the Windows leg against the shipping dist.
- **macOS.** Unmeasured, as before.
- **A rebuilt app.** The suite runs `engine::*` from the source tree and the shipping vips
  dist as sidecars; it does not build or launch the Tauri shell, so this proves the argv
  contract and the engine behaviour, not an installer.
- **The input bracket's `strip` token.** The loader slot still renders
  `in.jpg[fail-on=error,strip]` when stripping (it did before this card too, and the strip
  path is measured green with it). `strip` is a saver option, so it does nothing there; it
  was left alone deliberately — this card fixes the saver slot only, and changing a second
  thing in the same argv would blur which measurement answers which question.
