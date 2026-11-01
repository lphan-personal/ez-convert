# Q2 — licence audit evidence (LGPL compliance + engine pack)

Every number below is tool output captured in this run (2026-11-01/02, warden,
task `t_18165ce9`). The deliverable tables are `docs/SHIPPED-INVENTORY.md` and
`docs/shipped-inventory.tsv`; the gate is `scripts/dist-license-gate.sh`.

## 1. vips dist — 43/43 re-hashed against the S4 manifest

```
$ cd /opt/data/kanban/boards/ezconvert/workspaces/t_61e55863/dist/x
$ sha256sum -c <(grep -E '^\S+\s+[0-9]+\s+\./' spikes/s4/artifacts/FINAL-dist-spec-aom-sha256.txt | sed -E 's/\s+[0-9]+\s+/  /')
=== total checked: 43
43                 # 43 ': OK' lines, 0 mismatches
```

`libjbig-0.dll` (the S2-era GPL-2.0 contaminant) is absent from this dist and
from the manifest. Re-hashed independently on the shipping box afterwards
(win11-dev `C:\build\dist`, PowerShell `Get-FileHash`): **43/43 hash-identical
to the S4 manifest, 0 mismatches**. The only `jbig` strings inside the relinked
binaries are the *disabled-codec stubs*:

```
$ strings -a libtiff-6.dll | grep -i jbig
ISO JBIG            # codec-name table entry (tif_codec.c)
tif_jbig.c          # stub filename in the "not configured" path
$ strings -a libvips-42.dll | grep -iE 'jbig|poppler|imagequant|x26'
PDF load with PDFium or Poppler: false (dynamic module: false)
image quantisation with imagequant or quantizr: false
popplerload.c
```

## 2. vips dist — PE import closure over all 35 binaries

`objdump -p` per binary; the union of non-system imports is exactly the 32
dist DLLs (the 3 executables + 31 DLLs close over each other + `KERNEL32` /
`api-ms-win-crt-*` only). `jbig` in import tables: **0**. Banned names
(`libx26[45]`, `imagequant`, `poppler`, `magick`, `ghostscript`, `mupdf`) in
import tables: **0**.

## 3. ffmpeg builds — configuration asserted, not assumed

win64 (`C:\ezc\ffmpeg` on win11-dev, over SSH):

```
ffmpeg version N-128273-g836436cfb9-20261101 ...
configuration: ... --enable-version3 ... --disable-libx264 --disable-libx265 ...
(enable-gpl | enable-nonfree) occurrences in `ffmpeg -version`:  0
ffmpeg.exe  sha256 DA90280EA6334D65BA9082A007251022EA67B226543CACD3A9F7AAAB00338D52
ffprobe.exe sha256 894F68A5F40B628BC2FE0641B9100AE93161797063855C74ACBAB1258D75BDC4
```

linux64 (BtbN `ffmpeg-master-latest-linux64-lgpl`, local):

```
ffmpeg version N-128273-g836436cfb9-20261101 ...
configuration: ... --enable-version3 ... --disable-libx264 --disable-libx265 ...
enable-gpl/nonfree occurrences: 0
ffmpeg  sha256 cf745359b4e58b9a4e6f9821dccfe2109e64665631efd1738f8f0a7a5542eeb7
ffprobe sha256 f9af0178f3a1daf1993d1bdf26846aa0d5d4a1b8037fd97e4d3991b5f5e886ef
```

Contrast (NOT shipped): `/usr/local/bin/ffmpeg` on this host is a GPL build
(`--enable-gpl --enable-version3 ... --enable-libx264 --enable-libx265`); it is
outside every scan root by design.

## 4. LibreOffice — MPL, separate process, not linked

- Pack `LICENSE` line 3: "This product is made available subject to the terms of
  the Mozilla Public License, v. 2.0."
- No LO/UNO/soffice reference in `src-tauri/Cargo.toml` or `Cargo.lock`;
  `#[link]` attributes in the app: `user32`, `kernel32` only.
- `engine/soffice.rs` doc: "this module never constructs a `Command`" — argv for
  `super::proc::run`, spawned as `soffice.com`/`soffice` (ARCHITECTURE §6.2).
- Provenance (S3 evidence): TDF tarball `LibreOffice_26.8.2_Linux_x86-64_deb.tar.gz`
  (221,006,208 B), `.deb`s extracted with `dpkg-deb -x`. Windows build 26.8.2.2
  (S3b). NOTE: the tarball digest was not recorded at download time — stated gap.

## 5. Fonts / bundled assets — terms verified per family

Source: the pack's own `LICENSE.html` Fonts section (excerpted in the audit
log; families matched to `share/fonts/truetype/`). GPL-term findings: five
Culmus families `GPL-2.0-only WITH Font-exception-2.0` (no permissive arm);
Linux Libertine G/Biolinum G dual `GPL-2.0+FE OR OFL-1.0` (OFL elected).
Everything else OFL-1.1 / Apache-2.0 / Bitstream Vera / public domain.
`share/gallery/**` and `help/**` carry no statement in the pack's licence
documents → unverified, flagged (F2/F3), gate WARNs.

## 6. Gate runs (final state)

```
$ scripts/dist-license-gate.sh --check-inventory
inventory: 88 rows checked, 0 failures, 7 warnings      # 5x Culmus F1, 2x F2/F3
$ scripts/dist-license-gate.sh --selftest
selftest: PASS (clean tree accepted)
selftest: PASS (unattributed binary (the jbigkit shape) rejected)
selftest: PASS (GPL binary row rejected)
selftest: PASS (AGPL row rejected)
selftest: PASS (ffmpeg with --enable-gpl rejected)
selftest: PASS (real shipped-inventory.tsv accepted)
selftest: ALL PASS
$ scripts/dist-license-gate.sh --scan <dist x>            # 43 files, sha-checked
scan: 43 files attributed, 0 failures
$ scripts/dist-license-gate.sh --scan <LO pack opt/libreoffice26.8>
scan: 13979 files attributed, 0 failures
$ scripts/dist-license-gate.sh --scan <LO pack usr/>
scan: 258 files attributed, 0 failures
$ scripts/dist-license-gate.sh --scan <lo-deps>
scan: 21 files attributed, 0 failures
$ scripts/license-assert.sh --selftest
selftest 1/2: PASS (gate exited 1 on a GPL build)
selftest 2/2: PASS (gate exited 0 on an LGPL build)
$ python3 scripts/gen-licenses.py --check
docs/LICENSES.md is current
```

## 7. Known gaps (stated, not papered over)

- The 3-OS CI verdict for the pushed head is not observable from this container
  (Actions API 403, Contents-scoped token); the CI steps are the same commands
  run above.
- The Windows LibreOffice pack (26.8.2.2) was inventoried by component (same
  version line, same `LICENSE.html` family set); its per-file tree was not
  re-hashed here — only the Linux pack was walked file-by-file.
- The LO tarball digest was never recorded (see §4); tree-level verification is
  what exists.
- macOS engine pack (P3) does not exist yet.
