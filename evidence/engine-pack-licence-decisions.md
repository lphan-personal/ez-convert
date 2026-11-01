# Engine-pack licence decisions — evidence (card t_20fae747)

Scope: the three decisions the Q2 audit left open for the LibreOffice engine pack —
F1 (five Culmus font families, GPL-2.0), F2 (`share/gallery/**`), F3 (`help/**`).
Everything below is measured on this box, not inferred. Pack under test:
`vendor/libreoffice-26.8.2` (`opt/libreoffice26.8`, TDF 26.8.2 Linux x86-64 debs,
`soffice --version` = `LibreOffice 26.8.2.2 9dea406ab7695df6325b11b0e19fdca8db4b7276`),
run with `LD_LIBRARY_PATH=vendor/lo-deps/usr/lib/x86_64-linux-gnu` (the same
runtime libs the pack ships for Linux).

Verdicts (recorded on the card and in `docs/SHIPPED-INVENTORY.md` §6):

| finding | verdict |
|---|---|
| F1 five Culmus families | **STRIP** — GPL-2.0 with **no** font exception, and LO's PDF export embeds them |
| F2 `share/gallery/**` | **STRIP** — terms not itemisable; unused by headless conversion |
| F3 `help/**` | **VERIFIED (MPL-2.0 + permissive assets), then STRIP** — unused by headless conversion |

---

## 1. F1 — the card's premise was wrong: these five families have no font exception

The decision card (and the Q2 inventory row) recorded the five families as
`GPL-2.0-only WITH Font-exception-2.0`. The pack's own `LICENSE.html`
(Fonts → Culmus) does not support that for the families we ship.

Section opener (verbatim):

> Culmus — This package is distributed under the terms of GNU General Public
> License version 2.

Then per-family paragraphs. Measured with
`python3` over the extracted Culmus section: **6** occurrences of
"As a special exception, if you create a document which uses this font …", and
they sit under exactly six families — Shofar, Keter YG, Hadasim, Simple,
Stam Ashkenaz, Stam Sefarad (all Yoram Gnat's ancient-script fonts). The five
families in the pack carry a copyright line and "All rights reserved" with no
exception paragraph:

```
Digital version of David font family (Hebrew glyphs, numerals and part of punctuation) is
copyright 2002-2010 by Maxim Iorsh … Latin glyphs … are part of Bitstream Charter font
family and are copyright 1990 … See also LICENSE-BITSTREAM below:      <- no exception para
Frank-Ruehl font family (Hebrew glyphs) is copyright 2002-2011 by Maxim Iorsh …  <- none
Nachlieli font family (Hebrew glyphs) is copyright 2002-2017 by Maxim Iorsh …    <- none
Miriam font family is copyright 2004-2010 by Maxim Iorsh … All rights reserved.  <- none
Miriam Mono font family (Hebrew glyphs) is copyright 2002-2004 by Maxim Iorsh …  <- none
```

Programmatic check (same order as the HTML):

```
exception paragraphs in Culmus section: 6
David font family          exception-within-1200-chars=False
Frank-Ruehl font family    exception-within-1200-chars=False
Nachlieli font family      exception-within-1200-chars=False
Miriam font family         exception-within-1200-chars=False
Miriam Mono font family    exception-within-1200-chars=False
```

The upstream README has the same split — the exception paragraphs appear only
above the Gnat families, and the Iorsh families are followed by "All rights
reserved" (`https://github.com/Culmus/hebrew-fonts`). CTAN's package note says the
same thing in one line: *"The fonts are distributed under the terms of GNU
General Public License version 2, some of them with embedding exception."*
(`https://ctan.org/tex-archive/fonts/culmus`)

So the lawful-reading in the card — "mere aggregation, and the font exception
protects documents that embed them" — holds only for the *aggregation* half. The
documents half does not hold for the five families we ship.

## 2. F1 — what the product actually does with these fonts (the deciding measurement)

The app converts documents with `--convert-to pdf:writer_pdf_Export` etc.
(`src-tauri/src/engine/soffice.rs`, routes D1/D7/C3-*), i.e. **default PDF export
options** — and the default embeds the font programs.

Test document: a flat ODF (`evidence/fixtures/hebrew-clm.fodt`) with five
paragraphs, one per Culmus family, Hebrew text. Converted with the untouched pack:

```
$ soffice --headless --norestore --convert-to pdf --outdir out-full hebrew-clm.fodt
convert … -> out-full/hebrew-clm.pdf using filter : writer_pdf_Export

$ grep -a -o "/BaseFont */[A-Za-z0-9+#-]*" out-full/hebrew-clm.pdf | sort -u
/BaseFont/BAAAAA+DavidCLM-Medium
/BaseFont/CAAAAA+NotoSans-Regular
/BaseFont/DAAAAA+DejaVuSans
/BaseFont/EAAAAA+FrankRuehlCLM-Medium
/BaseFont/FAAAAA+MiriamCLM-Book
/BaseFont/GAAAAA+NotoSerif-Regular
/BaseFont/HAAAAA+MiriamMonoCLM-Book
/BaseFont/IAAAAA+NachlieliCLM-Light

$ grep -a -c "/FontFile" out-full/hebrew-clm.pdf
8
```

Five GPL-2.0 font programs, subsetted (a subset is a modified copy), embedded in a
PDF the user will send to other people — with no embedding permission in the
licence. That is the obligation the product would create for its own users, and it
is why the "keep + ship the GPL text and the sources" option does not cover it:
the GPL text and the source address *our* distribution of the font files, not the
user's redistribution of a PDF containing modified copies of them.

Same document, after the strip (§4):

```
/BaseFont/BAAAAA+NotoSerif-Regular
/BaseFont/CAAAAA+NotoSans-Regular
/BaseFont/DAAAAA+DejaVuSans
/BaseFont/EAAAAA+NotoSans-Regular
```

## 3. F1 — the OFL twins do not take over automatically

The pack keeps the OFL-1.1 twins (`DavidLibre`, `FrankRuhlHofshi`, `MiriamLibre`,
plus Alef / Noto Sans Hebrew / DejaVu), and `share/registry/main.xcd` even lists
`David CLM;David Libre;Frank Ruehl CLM;…` as a CTL font *preference* order. But
that is not a substitution mapping for a missing font: with the CLM files gone LO
picked Noto Serif / Noto Sans / DejaVu, not the twins.

An explicit mapping was tried and **measured to have no effect** on this version's
export path: five `FontSubstitutions` nodes were added to
`share/registry/main.xcd` (`davidclm → David Libre;…`, `nachlieliclm → Alef;…`,
etc.), the pack was re-run on the same document, and the PDF still used Noto
Serif / Noto Sans / DejaVu. Control: a document asking for `Agency FB` — which has
an existing shipped substitution to `Agdasima` in the same table — also rendered
in Noto Sans. So the table as patched is not what the export path consults, and a
pack-level patch is not a free fidelity win; the pack file was reverted to its
pristine digest (`b138b28d…`).

**Outcome (follow-up t_76ca844f, closed):** the mapping does work — just not from
the pack. LibreOffice keeps the `Tools ▸ Options ▸ Fonts` replacement table in the
*user profile* (`/org.openoffice.Office.Common/Font/Substitution`, a `Replacement`
flag plus pairs of `ReplaceFont`/`SubstituteFont`/`Always`), and the app already
writes a profile per batch. `engine/soffice.rs::Profile::seed_font_substitutions`
now seeds `<profile>/user/registrymodifications.xcu` before the spawn, with
`Always=true` — measured to be the load-bearing flag: the same pairs with
`Always=false` produced output byte-identical to no table at all (28,391 B, same
four fallback fonts), and `Always=true` put the twins in the PDF. Measured through
`engine::doc::convert` on the stripped pack, the five families come out as
`DavidLibre-`, `FrankRuhlHofshi-`, `MiriamLibre-`, `LiberationMono`, `Alef-` and
no fallback is left in the PDF. Full log: `evidence/hebrew-fidelity.md`.

One correction to this section as first written: the fixture used here
(`hebrew-clm.fodt`) sets `style:font-name` only, i.e. the **Western** slot — its
Hebrew runs resolved through the document's CTL default and were drawn in DejaVu
Sans even with the full pack. The CLM programs were still embedded and used (they
drew the Latin runs), so the licence finding above is unchanged; but the Hebrew
fidelity question only becomes measurable with a CTL fixture, which is what
`evidence/fixtures/hebrew-clm-ctl.{fodt,docx}` (added by t_76ca844f) provides.
With it, the full pack draws the Hebrew in the Culmus face — i.e. real Hebrew
documents were embedding GPL font software, which strengthens F1.

## 4. The strip: what it removes, and proof it changes nothing else

`scripts/strip-engine-pack.py` (new, idempotent, dry-run capable):

```
$ scripts/strip-engine-pack.py --pack <pack> --dry-run
strip-engine-pack: culmus      18 files  1,333,708 B
strip-engine-pack: gallery    175 files 13,556,254 B
strip-engine-pack: help      5334 files 11,167,640 B

$ scripts/strip-engine-pack.py --pack <pack> --manifest strip-manifest.tsv
strip-engine-pack: removed 5527 files
strip-engine-pack: manifest -> strip-manifest.tsv (5527 entries)

$ scripts/strip-engine-pack.py --pack <pack>        # second run
strip-engine-pack: culmus       0 files          0 B
… removed 0 files                                   # idempotent
```

Totals: **5,527 files / 26,057,602 B** (`culmus` 18 / 1,333,708; `gallery` 175 /
13,556,254; `help` 5,334 / 11,167,640). On-disk pack size 735 MB → 695 MB (small
files cost a block each). `share/gallery` and `help` are pruned entirely.

Document conversion matrix, untouched pack vs stripped pack, same fixtures, same
filter strings the app uses (`fixtures/`):

```
fixture          full_B    strip_B  delta_B  fonts (stripped pack)
sample.docx       26460      26460        0  Caladea-Regular,Carlito-Bold,NotoSerif-Regular,
sample.xlsx       26211      26211        0  Carlito,NotoSans-Regular,
sample.pptx       33957      33957        0  Carlito,LiberationSans,NotoSans-Regular,
sample.rtf        13100      13100        0  NotoSerif-Regular,
sample.doc        26460      26460        0  Caladea-Regular,Carlito-Bold,NotoSerif-Regular,
sample.ppt        33915      33915        0  Carlito,LiberationSans,NotoSans-Regular,
```

**Byte-identical output** in all six directions, with `share/gallery` and `help`
absent. The strip costs nothing for non-Hebrew documents; for Hebrew documents the
cost is the substitution recorded in §2.

## 5. F2 / F3 — what the upstream terms actually are

The pack's own licence documents say nothing about either tree:

```
$ grep -o -i "gallery|helpcontent|clip.?art" LICENSE.html LICENSE NOTICE | wc -l
0
$ grep -ail "licen|copyright" share/gallery/* | wc -l
0
```

Nor does the gallery content carry embedded terms (175 files; `.sdg` are binary
theme files, `.str`/`.thm`/`.sdv` XML, plus gifs and `.wav` sounds).

Upstream reading — Debian's `debian/copyright` for LibreOffice 26.8.1.1
(`https://pkg.kali.org/media/packages/libr/libreoffice/copyright-426.8.1.1-3`,
the Debian packaging file, which is the closest thing to a per-path licence map
that exists for the tree):

```
Files: *
Copyright: Copyright 2000, 2010 Oracle and/or its affiliates.
 Copyright (c) 2000, 2010 LibreOffice contributors and/or their affiliates.
License: MPL-2.0

Files: helpcontent2/help3xsl/normalize.css   … License: Expat
Files: helpcontent2/help3xsl/prism.js        … License: MIT
Files: helpcontent2/help3xsl/prism.css       … License: MIT
Files: helpcontent2/help3xsl/flexsearch*.js  … License: Apache-2.0
Files: helpcontent2/help3xsl/polyfills.js    … License: MIT / BSD-2-clause
```

i.e. `help/**` is **MPL-2.0** with four permissive asset exceptions — F3 is
verifiable, contrary to the Q2 note — and `extras/` (the gallery) falls under
`Files: * → MPL-2.0`, with no per-asset provenance for the historical clipart.

So: F3 is stripped as *unused*, not as unlicensed (the engine runs headless and
never opens a help viewer; 5,334 files / 11,167,640 B). F2 is stripped on both
counts: not itemised upstream, and unused.

## 6. Gate state after the decisions

```
$ scripts/dist-license-gate.sh --check-inventory
inventory: 89 rows checked, 0 failures, 0 warnings        # was 7 warnings

$ scripts/dist-license-gate.sh --selftest
selftest: PASS (clean tree accepted)
selftest: PASS (unattributed binary (the jbigkit shape) rejected)
selftest: PASS (GPL binary row rejected)
selftest: PASS (AGPL row rejected)
selftest: PASS (ffmpeg with --enable-gpl rejected)
selftest: PASS (file matching a not-shipped row rejected)
selftest: PASS (dir-qualified basename glob attributes its file accepted)
selftest: PASS (specific not-shipped row beats a broad shipped row rejected)
selftest: PASS (real shipped-inventory.tsv accepted)
selftest: ALL PASS

$ scripts/dist-license-gate.sh --scan <stripped pack>       # rc=0
scan: 8452 files attributed, 0 failures

$ scripts/dist-license-gate.sh --scan <untouched pack>      # rc=1
scan: 13979 files attributed, 5527 failures
   5334  lo-help                       (help/**)
    175  lo-gallery                    (share/gallery/**)
     18  lo-font-Culmus-*              (four families x4 files, MiriamCLM x2)
```

The new rule is the point: before this card, marking rows `not-shipped` while the
files stayed in the tree would have passed the gate silently. Now the strip
decision is enforced by the same fail-closed mechanism as the `libjbig-0.dll`
case.

The per-family font rows also needed a matcher fix to be enforceable at all: a
row pattern like `share/fonts/truetype/DavidCLM-*` was previously matched only
against the *basename* (so it never hit) or against the whole relative path, and
the file fell through to the shorter `share/*` row — i.e. a re-added Culmus font
would have been attributed to in-tree MPL content and passed. Directory-qualified
patterns whose last segment is a glob now match prefix + basename, and the longest
pattern wins, which is what makes the 18 Culmus FAILs above appear at all. Two
selftest cases pin both halves (attribution, and the specific row beating the
broad one).

## 7. Capability loss and follow-up

- Documents that reference `David CLM`, `FrankRuehl CLM`, `Miriam CLM`,
  `Miriam Mono CLM` or `Nachlieli CLM` render in Noto Serif / Noto Sans /
  DejaVu **unless** the app's profile seed is in place (§3 outcome). Hebrew text
  stays correct and readable either way; glyph shapes and metrics differ from the
  Culmus originals.
- Follow-up **t_76ca844f — done**: the five families are mapped onto shipped
  OFL-1.1 twins through the LibreOffice **user profile**
  (`registrymodifications.xcu` written into the engine's temp profile before the
  spawn, `Always=true`), because the pack-level `FontSubstitutions` route is
  measured ineffective (§3). The mapping is `David CLM → David Libre`,
  `Frank Ruehl CLM → Frank Ruhl Hofshi`, `Miriam CLM → Miriam Libre`,
  `Miriam Mono CLM → Liberation Mono`, `Nachlieli CLM → Alef`; measured through
  `engine::doc::convert` on the stripped pack, the PDF carries the twins and no
  fallback. No pack file is modified, and no font is added: the twins already
  ship. One behaviour change worth knowing: `Always=true` substitutes even when a
  family is installed, so the product's output is OFL-only regardless of the
  host's fonts (`Always=false` is inert — evidence §1).
- Nothing else in the pack changed: the conversion matrix is byte-identical (§4)
  and the gate is green on the stripped tree (§6).
