# Hebrew fidelity after the Culmus strip — evidence (card t_76ca844f)

Scope: restore the glyph/metrics fidelity the licence strip (card t_20fae747, F1)
removed, using only fonts that still ship. Everything below is measured on this
box; nothing is inferred from documentation.

Pack under test: a stripped copy of `vendor/libreoffice-26.8.2`
(`opt/libreoffice26.8`, TDF 26.8.2.2, `soffice --version` = `LibreOffice
26.8.2.2 9dea406ab7695df6325b11b0e19fdca8db4b7276`) produced by the committed
`scripts/strip-engine-pack.py` — 8,452 files, and `share/fonts/truetype` holds
**0** files matching `*CLM*` — run with
`LD_LIBRARY_PATH=vendor/lo-deps/usr/lib/x86_64-linux-gnu` (the same runtime
libs the pack ships), and `share/registry/main.xcd` at its pristine digest
`b138b28d4b826b68b08b17fce8432794` (**no pack file was modified**: the earlier
`FontSubstitutions` experiment was reverted before these runs).

The mechanism is in app code (`src-tauri/src/engine/soffice.rs`
`Profile::seed_font_substitutions`, called from `engine/doc.rs` before the
spawn), and it is measured through the shipped pipeline, not by hand-running
`soffice` (see §4).

---

## 1. The mechanism: the profile's font-replacement table, with `Always=true`

LibreOffice keeps the `Tools ▸ Options ▸ LibreOffice ▸ Fonts` replacement table in
the *user profile*, not in the pack: path
`/org.openoffice.Office.Common/Font/Substitution`, a boolean `Replacement` plus a
set `FontPairs` of `{ReplaceFont, SubstituteFont, OnScreenOnly, Always}` nodes
(schema read from the pack's own `share/registry/main.xcd`). The file LibreOffice
reads at startup is `<profile>/user/registrymodifications.xcu`.

Probe: the five-family fixture converted on the stripped pack
(`soffice --headless --norestore --nolockcheck --nodefault --nologo
-env:UserInstallation=file://<tmp>/profile --convert-to pdf:writer_pdf_Export`),
with the seed written into `<tmp>/profile/user/` **before** the first start.

| seed | PDF bytes | `/BaseFont` in the produced PDF |
|---|---|---|
| none | 28,391 | `NotoSerif-Regular`, `NotoSans-Regular`, `DejaVuSans`, `NotoSans-Regular` |
| `Replacement=true`, pairs with `Always=false` | 28,391 | **identical to the row above** — the table did nothing |
| `Replacement=true`, pairs with `Always=true` | 41,634 | `DavidLibre-Regular`, `FrankRuhlHofshi-Regular`, `MiriamLibre-Regular`, `LiberationMono`, `Alef-Regular` (+ `NotoSans-Regular`, `DejaVuSans` for the Latin label of the ODF fixture — see §2) |

So `Always` is the load-bearing flag. In this version a pair with
`Always=false` is not consulted by the PDF export path at all — the output is
byte-for-byte the no-seed output (28,391 B, same four `BaseFont` names). The
consequence is deliberate and is recorded here rather than hidden:
**`Always=true` substitutes even when the family is installed.** Measured on the
*unstripped* pack (Culmus present): the same conversion produced the same five
twins and the same 58,470 B PDF as the stripped pack. The product's output is
therefore OFL-only whatever the host has installed, and a user with Culmus
installed sees the twins rather than the GPL families.

Two more properties, because the app's profile is long-lived
(`<cache>/doc/profile`, reused across batches) and cannot be assumed fresh:

- **Merge, not overwrite.** With a profile LibreOffice had already bootstrapped
  (first run without the seed, then the seed merged into the file it wrote), the
  next conversion produced the twins — the same 7 fonts, 41,634 B.
- **Idempotent.** LibreOffice rewrites `registrymodifications.xcu` on exit and
  keeps items it did not originate; it does reorder the properties inside an
  item. The seed re-reads the file and adds only what is missing, so repeated
  runs never duplicate the table (pinned by unit test).

## 2. Fixture correction: the ODF fixture exercised the Western slot, not the CTL slot

The committed `evidence/fixtures/hebrew-clm.fodt` sets `style:font-name` per
paragraph and nothing else. In ODF that is the **Western** font; a Hebrew run is
resolved through the *CTL* font, which that fixture never set. Measured by
reading back which font drew which run (a throwaway PDF parser over the content
stream and each font's `ToUnicode` CMap):

```
full pack (Culmus present), hebrew-clm.fodt
  'P1 David CLM: '      DavidCLM-Medium
  ' ,םלוע םולש'          NotoSans-Regular + DejaVuSans      <- the Hebrew, in DejaVu
  '1234'                NotoSans-Regular
```

So the strip's *licence* finding stands unchanged (the CLM programs were
embedded and used — for the Latin runs), but the fixture could not show a
*Hebrew* fidelity loss: its Hebrew never came from Culmus. Two fixtures fix
that, both committed:

| fixture | bytes | sha256 | sets |
|---|---|---|---|
| `evidence/fixtures/hebrew-clm-ctl.fodt` | 3,107 | `9b630e0ab57eb8f33e8879c8fd8118155a78e1c3472864c9f60095ff6a50f8ff` | `style:font-name-complex` = the CLM family |
| `evidence/fixtures/hebrew-clm-ctl.docx` | 5,341 | `05fea9a2452e307d389d112718fd92a37fac69aaa156f4c2aba4e14b0acc2f00` | `w:rFonts w:ascii/w:hAnsi/w:eastAsia/w:cs` = the CLM family |

The `.docx` is the pinned LibreOffice 26.8.2 converting the `.fodt`
(`--convert-to docx:"MS Word 2007 XML"`); it is the fixture the probe converts,
because `.fodt` is not a canonical type in `model::types` and the app's document
route for this shape is `Docx -> Pdf` (D1).

With the CTL fixture on the **full** pack, the Hebrew is in the Culmus face —
this is the behaviour the strip removed:

```
full pack, hebrew-clm-ctl.docx
  P1 (David CLM)        DavidCLM-Medium      (label, Hebrew, digits)
  P2 (FrankRuehl CLM)   FrankRuehlCLM-Medium
  P3 (Miriam CLM)       MiriamCLM-Book       (its Latin label falls to NotoSerif — Miriam has no Latin)
  P4 (Miriam Mono CLM)  MiriamMonoCLM-Book
  P5 (Nachlieli CLM)    NachlieliCLM-Light
```

Stripped pack, **no** seed — one paragraph, three fonts, none of them a Hebrew
design; the Hebrew falls to Linux Libertine G:

```
  P1 label   NotoSerif-Regular
  P1 Hebrew  NotoSans-Regular + LinuxLibertineG
  P1 digits  NotoSans-Regular
  P4 label   DejaVuSansMono
  P5 label   NotoSans-Regular
```

Stripped pack, **with** the seed — every run of every paragraph in its twin:

```
  P1 label/Hebrew/digits   DavidLibre-Regular
  P2 label/Hebrew/digits   FrankRuhlHofshi-Regular
  P3 label/Hebrew/digits   MiriamLibre-Regular
  P4 label/Hebrew/digits   LiberationMono
  P5 label/Hebrew/digits   Alef-Regular
```

## 3. Which twin — chosen by advance width, not by name

Advance width of the fixture's Hebrew line (`שלום עולם, 1234`) in em units,
computed from each font's `cmap` + `hmtx` in the pack
(`sum(advance)/unitsPerEm`, 10 non-space code points). Δ is against the family
being replaced.

| stripped family (width) | candidate | width | Δ | note |
|---|---|---|---|---|
| David CLM `6.3783` | **David Libre** | 6.8853 | **+8.0 %** | the same design (Libre) |
| | Noto Serif Hebrew | 7.0890 | +11.1 % | different design |
| Frank Ruehl CLM `6.2475` | **Frank Ruhl Hofshi** | 6.5240 | **+4.4 %** | the same design |
| | Noto Serif Hebrew | 7.0890 | +13.5 % | |
| Miriam CLM `6.6312` | **Miriam Libre** | 6.9160 | **+4.3 %** | the same design |
| | Noto Serif Hebrew | 7.0890 | +6.9 % | |
| Miriam Mono CLM `9.0000` | **Liberation Mono** | 9.0015 | **+0.02 %** | monospaced **and** 27/27 Hebrew |
| | Noto Sans Hebrew | 7.4110 | −17.7 % | loses the monospace axis |
| Nachlieli CLM `7.0367` | **Alef** | 6.7529 | **−4.0 %** | the pack's OFL Hebrew sans |
| | Noto Sans Hebrew | 7.4110 | +5.3 % | |
| | Rubik | 6.9980 | −0.6 % | closer in width, but a geometric face — wrong lineage for an Arial-lineage sans |

Hebrew coverage checked per candidate over the 27 U+05D0–U+05EA letters: Alef,
David Libre, Frank Ruhl Hofshi, Miriam Libre, Liberation Mono, Noto Sans Hebrew
and Noto Serif Hebrew all cover 27/27. **DejaVu Sans Mono does not** (0/27) — it
was the first pick and it was wrong: the substitution went through, and the
Hebrew then fell back glyph-by-glyph, which is worse than no mapping at all. That
is why the mono twin is Liberation Mono.

## 4. The result through the shipped pipeline

`scripts/hebrew-fidelity.sh` compiles `e2e/hebrew/driver.rs` against the shipped
module tree (the same `#[path]` wiring `scripts/test-engine.sh` and
`scripts/e2e.sh` use) and calls `engine::doc::convert` — argv, profile seed,
publication and validation are the shipped ones. Verbatim, on the stripped pack:

```
$ bash scripts/hebrew-fidelity.sh /opt/data/cache/scratch/lo-strip/libreoffice26.8
pack:            /opt/data/cache/scratch/lo-strip/libreoffice26.8
culmus files:    0 (must be 0)
--- convert (engine::doc::convert, Docx -> Pdf)
produced: …/hebrew-clm.pdf (58470 B)
--- /BaseFont in the produced PDF
    BAAAAA+DavidLibre-Regular
    CAAAAA+DavidLibre-Regular
    DAAAAA+FrankRuhlHofshi-Regular
    EAAAAA+FrankRuhlHofshi-Regular
    FAAAAA+MiriamLibre-Regular
    GAAAAA+MiriamLibre-Regular
    HAAAAA+LiberationMono
    IAAAAA+LiberationMono
    JAAAAA+Alef-Regular
    KAAAAA+Alef-Regular
PASS  DavidLibre- present
PASS  FrankRuhlHofshi- present
PASS  MiriamLibre- present
PASS  LiberationMono present
PASS  Alef- present
RESULT PASS  the five twins replace the stripped families in the PDF
```

Ten subsets for five families because LibreOffice emits a separate subset for the
Hebrew run and the Latin run, both resolving to the same twin. The script fails
if any of `NotoSerif-Regular`, `LinuxLibertine`, `NotoSans-Regular`,
`DejaVuSansMono` is still embedded — the four fallbacks measured in the no-seed
runs — and it refuses to run against a pack that still carries Culmus files,
because with the families installed LibreOffice would use them and the check
would pass for the wrong reason.

## 5. Regression — non-Hebrew unchanged, document grid green

```
$ bash scripts/e2e.sh --docs-only        # stripped pack, shipped code path
advertised=48 pass=48 fail=0 skip=0 controls=0/0 caught
SUMMARY GREEN  every advertised direction passed on this dist
family     advertised   pass   fail   skip
document           42     42      0      0
```

`docx->pdf` in that grid: **26,460 B** — the same byte count the pack produced
before the seed existed (card t_20fae747 §4 table) and the same font set:

```
$ <driver> fixtures/sample.docx …          # through engine::doc::convert, seed active
26460 B   BaseFont: Carlito-Bold, Caladea-Regular, NotoSerif-Regular
```

Unit tests: `bash scripts/test-engine.sh` → **193 passed, 0 failed**, including six
new ones that pin the table (§ `every_stripped_family_maps_to_a_shipped_ofl_twin`),
the file location (`the_seed_is_written_where_libreoffice_reads_it`), idempotency
(`seeding_twice_does_not_duplicate_the_table`), merging into a profile LibreOffice
already wrote (`seeding_merges_into_a_profile_libreoffice_already_wrote`), the
post-rewrite recognition (`a_rewritten_config_is_recognised_as_already_seeded`)
and the call site (`a_document_batch_seeds_the_font_replacement_table_into_its_profile`).

## 6. What this does not cover

- **The Windows leg is not measured here.** The profile file, its path
  (`<profile>/user/registrymodifications.xcu`) and the config keys are identical
  on Windows; nothing in the mechanism is platform-specific. The packaged
  installer still has to be exercised on Windows — that is P2/forge's leg, not
  this card's.
- **`Always=true` overrides an installed family** (§1). No middle ground exists
  in 26.8: `Always=false` is inert for this export path, so the choice is
  twin-always or no fidelity at all.
- **`.fodt` is not a shipped input type.** The probe therefore converts the
  `.docx` twin of the fixture; the `.fodt` is kept as the readable source of it.
- **Hebrew text extraction order.** The produced PDFs are RTL documents, so the
  text a PDF tool extracts is in visual order (`םלוע םולש`); that is the
  engine's normal output and is not affected by the substitution.
