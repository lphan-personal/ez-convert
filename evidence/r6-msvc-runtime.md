# R6 — the eight MSVC runtime DLLs in the licence inventory + notices (t_34f3a93b)

Closes the R6 debt recorded by P2 (`evidence/p2-windows-installer.md` §7): the Windows NSIS
installer ships the VC143 CRT runtime family because `bundle.windows.bundleVCRuntime` is
`true`, and the files had no `docs/shipped-inventory.tsv` row and no
`docs/licenses/THIRD-PARTY-NOTICES.md` entry. Every number below is tool output; the VM is
win11-dev (`ssh -i /opt/data/.ssh/win11-dev ryo@10.0.0.191`, cmd.exe).

## 1. The gap, in gate terms (before)

`bash scripts/dist-license-gate.sh --scan` over a stand-in of the install root
(eight DLLs + `ez-convert.exe` + `uninstall.exe` + `licenses/THIRD-PARTY-NOTICES.md`, all
copied from the installed tree):

```
FAIL unattributed file in dist: <each of the 8 DLLs> (no row in shipped-inventory.tsv)
FAIL unattributed file in dist: ez-convert.exe (no row in shipped-inventory.tsv)
FAIL unattributed file in dist: uninstall.exe (no row in shipped-inventory.tsv)
scan: 11 files attributed, 10 failures        (exit 1)
```

`ez-convert.exe` and `uninstall.exe` were equally unattributed (row 14 is a component row,
its path field is prose, not a scan pattern — the same gap P3 filled for the DMG with
`app-binary-mac`), so closing the install root needed two `app` rows in addition to the
eight runtime rows.

## 2. Measured digests (2026-11-02)

`certutil -hashfile <file> SHA256` over both `C:\p2_offdev\vcredist\14.44.35211\x64\Microsoft.VC143.CRT\`
(the redist tree P2 bundled from) and the installed tree `%LOCALAPPDATA%\EZ Convert\`:
byte-identical in both locations, and each digest cross-checked locally with `sha256sum`
after copying the files out. Sizes match the P2 acceptance record.

| file | size (B) | sha256 |
|---|---|---|
| `concrt140.dll` | 324,208 | `2405355f0a58067b258f8df33c327e3a3d716eaac5a3a5aebb757842d85bd376` |
| `msvcp140.dll` | 557,728 | `0f885b509a685d2bbfa652fed26b5fb31d88fbdab0a978c641d1c7b8aa460aa9` |
| `msvcp140_1.dll` | 35,952 | `bfad5aef4c63a669e3c140655cdfdf395b6c979b400a447bd5dcb65ed8826c3d` |
| `msvcp140_2.dll` | 280,200 | `3ea06f0ee098b4823cb79599df3780e7f23cce52c19aac31d2a0d47efe33a5e9` |
| `msvcp140_atomic_wait.dll` | 50,304 | `640b2aefced484d0368eea5bdd06addd0658a3a70a49256e560d6923b404a479` |
| `msvcp140_codecvt_ids.dll` | 31,872 | `f2069a52880ec885ee7f0511186100eb7fada0411a2b4948fafea7735b878a18` |
| `vcruntime140.dll` | 124,544 | `d5e4d9a3e835fa679450145d6a7d94e36573a509317111904d9b3712c30d9066` |
| `vcruntime140_1.dll` | 49,792 | `1f2d41c4aa5db0bc33ebf7b66d72943a817d7ce6cbe880502a9403823633093f` |

Also verified against the installed bytes (rows that already existed, pins unchanged):
`ffmpeg.exe` `da90280ea6334d65ba9082a007251022ea67b226543cacd3a9f7aaab00338d52` and
`ffprobe.exe` `894f68a5f40b628bc2fe0641b9100ae93161797063855c74acbab1258d75bdc4` — both
match rows 19/20 exactly.

## 3. What changed in the repo

- `docs/shipped-inventory.tsv`: eight `binary` rows (`MSVC-runtime`, `14.44.35211`, bare
  basenames, `Microsoft Visual C++ Redistributable terms`, `status: shipped`, sha256-pinned)
  plus `app-binary-win` (`ez-convert.exe`) and `app-uninstaller-win` (`uninstall.exe`).
- `docs/licenses/THIRD-PARTY-NOTICES.md` §9: the redistributable notice — files, digests,
  and the operative Visual Studio 2022 REDIST clauses (quoted from Microsoft Learn,
  *Visual Studio 2022 Redistribution* — "you may copy and distribute with your program the
  unmodified form of the files listed below", the `[VisualStudioFolder]\VC\redist` clause,
  and the licensed-users clause for the redistributable package).
- `scripts/gen-licenses.py` + `docs/LICENSES.md` (regenerated, `--check` = current): one
  "MSVC runtime" row in the Bundled engines register — R6 says licences are additive in
  `docs/LICENSES.md`, and that file is generator-owned, so the row lives in the template.
- `docs/shipped-inventory.tsv` notices row re-pinned to three deliberate snapshots (below).

## 4. The notices row (hotspot, shared with the macOS pack)

The install tree carries four copies of `licenses/THIRD-PARTY-NOTICES.md`, measured:

| location in the install tree | sha256 | pin |
|---|---|---|
| `licenses\THIRD-PARTY-NOTICES.md` | `fb4d5357…` | committed bytes at P2/P3 time (what the current installer carries) |
| `engines\vips\licenses\THIRD-PARTY-NOTICES.md`, `licenses\vips\THIRD-PARTY-NOTICES.md`, `licenses\vips\licenses\THIRD-PARTY-NOTICES.md` | `fc485f87…` | the vips dist's snapshot (assembled before the notices changed) |

The row now records three digests, all deliberate:
`fc485f87…` (vips dist snapshot), `fb4d5357…` (P2/P3-era committed bytes), and
`900d2aef8c119d712e78dd07a1df27f95f57e693cd54ea15c02ae8870ee48c1d` (the committed bytes
after the §9 notice, this change). Scan fixtures, one per variant, against the re-pinned
row — the two pre-existing paths still pass and the new committed bytes pass:

| fixture (licenses/THIRD-PARTY-NOTICES.md =) | scan result |
|---|---|
| vips-dist snapshot `fc485f87…` (copied out of the installed tree) | 1 attributed, 0 failures |
| P2/P3 committed bytes `fb4d5357…` (git HEAD before this change) | 1 attributed, 0 failures |
| new committed bytes `900d2aef…` | 1 attributed, 0 failures |
| tampered fixture | 1 attributed, **1 failures** (fail-closed preserved) |

Rebuilding the vips dist and re-staging the packs collapses the row back to one digest —
same open item P2 recorded, unchanged.

## 5. Gate results (after)

```
--check-inventory:  inventory: 119 rows checked, 0 failures, 0 warnings
--selftest:         selftest: ALL PASS
--scan <stand-in>:  scan: 11 files attributed, 0 failures   (exit 0)
gen-licenses.py --check: docs/LICENSES.md is current
```

The stand-in is the install-root level: the eight DLLs, `ez-convert.exe`, `uninstall.exe`
and `licenses/` (the `licenses/**` catch-all attributing `EZ-CONVERT-DEPENDENCIES.md` is
proven by the real installed copy). The engine subtrees are covered by their own per-root
scans (P2 §2: vips dist 43, soffice pack 8327, sidecar 2 — all still green with the new
rows, the eight basenames appear nowhere else in the tree; measured `dir /s /b` over the
whole install tree returns exactly the eight root copies, so no bare-basename row can
shadow `program/**` or `engines/vips/**`).

## 6. Notes

- P2 §7 prose says "seven Microsoft VC-runtime DLLs"; the measured set is **eight** (the
  size lists in the P2 card and this evidence already enumerate eight files; the tree
  confirms eight).
- PE import surface, measured (`grep -a` over the shipped binaries): neither
  `ez-convert.exe` (UCRT API-set imports only) nor `uninstall.exe` (system DLLs only)
  names any of the eight DLLs, and `findstr` over `ffmpeg.exe`/`ffprobe.exe` found no
  `vcruntime140`/`msvcp140` reference either. The notices text therefore states the
  documented purpose of `bundleVCRuntime` (quoted from `docs/PACKAGING-WINDOWS.md`) and
  makes no claim about which component loads the DLLs.
- The install tree duplicates the vips licence set under `licenses\vips\` (with a nested
  `licenses\` inside it) in addition to `engines\vips\licenses\`. Harmless for the gate
  (`licenses/**` catch-all, notice-text), but it is redundant payload worth collapsing
  when the vips dist is rebuilt — recorded, not fixed here.
