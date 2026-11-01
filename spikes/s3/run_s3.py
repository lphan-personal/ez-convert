#!/usr/bin/env python3
"""S3 spike driver — LibreOffice document direction matrix.

Proves (or disproves) every document direction the app needs, one direction at
a time, with the output file asserted on disk *and* its content checked.
A conversion that exits 0 while producing no file is recorded as FAIL, never
as a pass -- that failure mode is the reason this spike exists.

    python3 run_s3.py --soffice /path/to/soffice --out out [--reps 3]
                      [--ld-library-path DIR] [--controls]

Writes out/result.json, out/table.md, out/logs/*, out/products/* and exits
non-zero only if a direction marked required fails.

Filter-name rule proved by this spike: a filter name containing spaces is ONE
argv element with NO quote characters. `--convert-to 'csv:"Text - txt - csv
(StarCalc)"'` passed to a shell-less exec (Rust Command, Python subprocess)
hands LibreOffice literal quote characters and it answers rc=1 "no export
filter". That is one of the two known ways to get no output file.
"""
import argparse
import csv
import hashlib
import json
import os
import platform
import shutil
import statistics
import subprocess
import sys
import time

import fixtures

HERE = os.path.dirname(os.path.abspath(__file__))
RUN_TIMEOUT = 240

CSV_INFILTER = "csv:44,34,76,1,,0,false,true,true"
CSV_OUTFILTER = "csv:Text - txt - csv (StarCalc):44,34,76,1,,0,false,true,true,false,false,1"


# --------------------------------------------------------------------------- utils
def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def human(ms):
    return "%.0f ms" % ms if ms is not None else "n/a"


# ------------------------------------------------------------------- soffice calls
def invoke(soffice, infile, outdir, convert_arg, infilter, profile, run_id, logs,
           ld_library_path=None, home=None):
    """Run one soffice conversion. Returns a record dict."""
    argv = [
        soffice,
        "--headless",
        "--norestore",
        "--nolockcheck",
        "--nodefault",
        "--nologo",
        "-env:UserInstallation=file://" + profile,
    ]
    if infilter:
        argv.append("--infilter=" + infilter)
    argv += ["--convert-to", convert_arg, "--outdir", outdir, infile]

    env = dict(os.environ)
    if home:
        env["HOME"] = home
    if ld_library_path:
        env["LD_LIBRARY_PATH"] = ld_library_path
    env["SAL_USE_VCLPLUGIN"] = "svp"
    env["SAL_DISABLE_OPENCL"] = "1"
    env.pop("DISPLAY", None)

    started = time.monotonic()
    try:
        proc = subprocess.run(
            argv, capture_output=True, text=True, timeout=RUN_TIMEOUT, env=env
        )
        rc, out, err = proc.returncode, proc.stdout, proc.stderr
        timed_out = False
    except subprocess.TimeoutExpired as exc:
        rc, timed_out = -9, True
        out = exc.stdout if isinstance(exc.stdout, str) else (exc.stdout or b"").decode("utf-8", "replace")
        err = exc.stderr if isinstance(exc.stderr, str) else (exc.stderr or b"").decode("utf-8", "replace")
    wall_ms = (time.monotonic() - started) * 1000.0

    with open(os.path.join(logs, run_id + ".stdout.txt"), "w", encoding="utf-8") as fh:
        fh.write(out)
    with open(os.path.join(logs, run_id + ".stderr.txt"), "w", encoding="utf-8") as fh:
        fh.write(err)

    return {
        "run_id": run_id,
        "argv": argv,
        "rc": rc,
        "timed_out": timed_out,
        "wall_ms": round(wall_ms, 1),
        "stdout": out.strip()[:2000],
        "stderr": err.strip()[:2000],
    }


# ----------------------------------------------------------------------- checkers
def check_pdf(path, markers, min_pages=1, page_size=None):
    from pypdf import PdfReader

    details, ok = [], True
    with open(path, "rb") as fh:
        head = fh.read(5)
    if head != b"%PDF-":
        ok = False
        details.append("header=%r (expected %%PDF-)" % head)
    else:
        details.append("header=%PDF-")
    reader = PdfReader(path)
    pages = len(reader.pages)
    details.append("pages=%d" % pages)
    if pages < min_pages:
        ok = False
    text = "\n".join((p.extract_text() or "") for p in reader.pages)
    for m in markers:
        found = m in text
        details.append("text[%s]=%s" % (m, "yes" if found else "NO"))
        ok = ok and found
    box = reader.pages[0].mediabox
    w, h = round(float(box.width), 1), round(float(box.height), 1)
    details.append("page0=%.1fx%.1fpt" % (w, h))
    if page_size:
        want_w, want_h = page_size
        if not (abs(w - want_w) < 3 and abs(h - want_h) < 3):
            ok = False
            details.append("page size mismatch (expected %sx%s)" % (want_w, want_h))
    return ok, details, {"pages": pages, "page0_pt": [w, h], "text_chars": len(text)}


def check_docx_html(path, markers):
    with open(path, "r", encoding="utf-8", errors="replace") as fh:
        text = fh.read()
    low = text.lower()
    details, ok = [], True
    for m in markers:
        found = m in text
        details.append("text[%s]=%s" % (m, "yes" if found else "NO"))
        ok = ok and found
    for tid, needle in (("<h1", "<h1"), ("<table", "<table"), ("<p", "<p")):
        found = needle in low
        details.append("%s=%s" % (tid, "yes" if found else "NO"))
        ok = ok and found
    pcount = low.count("<p")
    details.append("p_tags=%d" % pcount)
    ok = ok and pcount >= 3
    return ok, details, {"p_tags": pcount, "chars": len(text)}


def check_html_docx(path, markers):
    from docx import Document

    doc = Document(path)
    texts = [p.text for p in doc.paragraphs]
    blob = "\n".join(texts)
    details, ok = [], True
    for m in markers:
        found = m in blob
        details.append("text[%s]=%s" % (m, "yes" if found else "NO"))
        ok = ok and found
    details.append("paragraphs=%d" % len(texts))
    details.append("tables=%d" % len(doc.tables))
    ok = ok and len(texts) >= 3 and len(doc.tables) >= 1
    if doc.tables:
        rows = len(doc.tables[0].rows)
        cols = len(doc.tables[0].columns)
        details.append("table0=%dx%d" % (rows, cols))
        ok = ok and rows >= 2 and cols >= 2
        cellblob = " ".join(c.text for r in doc.tables[0].rows for c in r.cells)
        details.append("table_marker=%s" % ("yes" if markers[0] in cellblob else "NO"))
        ok = ok and markers[0] in cellblob
    return ok, details, {"paragraphs": len(texts), "tables": len(doc.tables)}


def check_csv_to_xlsx(path):
    from openpyxl import load_workbook

    wb = load_workbook(path)
    ws = wb.worksheets[0]
    details, ok = [], True
    details.append("sheets=%s" % wb.sheetnames)
    details.append("dims=%dx%d" % (ws.max_row, ws.max_column))
    want = {
        "A1": "id",
        "B1": "name",
        "C1": "amount",
        "D1": "note",
        "B2": "Widget A",
        "D2": "comma, inside",
        "D3": "caf\u00e9",
    }
    for coord, expected in want.items():
        got = ws[coord].value
        good = got == expected
        details.append("%s=%r %s" % (coord, got, "ok" if good else "MISMATCH exp %r" % expected))
        ok = ok and good
    for coord, expected in (("A2", 1), ("C2", 12.5), ("C3", -3.75), ("C4", 1000)):
        got = ws[coord].value
        good = isinstance(got, (int, float)) and abs(float(got) - float(expected)) < 1e-9
        details.append("%s=%r numeric %s" % (coord, got, "ok" if good else "MISMATCH exp %r" % expected))
        ok = ok and good
    ok = ok and ws.max_row >= 4 and ws.max_column >= 4
    return ok, details, {"sheets": wb.sheetnames, "rows": ws.max_row, "cols": ws.max_column}


def check_xlsx_to_csv(path, extra_sheet_marker=None):
    details, ok = [], True
    with open(path, "r", encoding="utf-8", errors="replace", newline="") as fh:
        raw = fh.read()
    rows = list(csv.reader(raw.splitlines()))
    details.append("rows=%d cols=%s" % (len(rows), [len(r) for r in rows]))
    header = rows[0] if rows else []
    details.append("header=%s" % header)
    ok = ok and header[:4] == ["id", "name", "amount", "note"]
    ok = ok and len(rows) >= 4

    def cell(r, c):
        return rows[r][c] if len(rows) > r and len(rows[r]) > c else None

    for r, c, expected in ((1, 1, "Widget A"), (1, 3, "comma, inside"), (2, 3, "caf\u00e9")):
        got = cell(r, c)
        good = got == expected
        details.append("r%dc%d=%r %s" % (r + 1, c + 1, got, "ok" if good else "MISMATCH exp %r" % expected))
        ok = ok and good
    for r, c, expected in ((1, 2, 12.5), (2, 2, -3.75), (3, 2, 1000.0)):
        got = cell(r, c)
        try:
            good = abs(float(got) - expected) < 1e-9
        except (TypeError, ValueError):
            good = False
        details.append("r%dc%d=%r numeric %s" % (r + 1, c + 1, got, "ok" if good else "MISMATCH exp %s" % expected))
        ok = ok and good
    if extra_sheet_marker:
        details.append("second-sheet marker in output=%s"
                       % ("yes" if extra_sheet_marker in raw else "no"))
    return ok, details, {"rows": len(rows)}


def check_docx_md(path, markers):
    with open(path, "r", encoding="utf-8", errors="replace") as fh:
        text = fh.read()
    details, ok = [], True
    for m in markers:
        found = m in text
        details.append("text[%s]=%s" % (m, "yes" if found else "NO"))
        ok = ok and found
    has_heading = text.lstrip().startswith("#") or "\n# " in text
    details.append("markdown_heading=%s" % ("yes" if has_heading else "NO"))
    ok = ok and has_heading
    return ok, details, {"chars": len(text)}


# ------------------------------------------------------------------------ matrix
# Each attempt is (label, convert_arg, infilter). The FIRST attempt whose output
# exists, is non-empty AND passes the content check wins the direction.
DIRECTIONS = [
    {
        "id": "docx2pdf",
        "label": "docx -> pdf",
        "src": "sample.docx",
        "out_suffix": ".pdf",
        "required": True,
        "attempts": [
            ("pdf:writer_pdf_Export", "pdf:writer_pdf_Export", None),
            ("pdf (bare)", "pdf", None),
        ],
        "check": lambda p: check_pdf(p, [fixtures.MARK_DOCX, "caf\u00e9", "Marker"], min_pages=1),
    },
    {
        "id": "docx2md",
        "label": "docx -> md",
        "src": "sample.docx",
        "out_suffix": ".md",
        "required": False,
        "attempts": [
            ("md:Markdown", "md:Markdown", None),
            ("Markdown", "Markdown", None),
            ("md (bare)", "md", None),
            ('md:"Markdown"', 'md:"Markdown"', None),
            ("txt:Text (encoded):UTF8", "txt:Text (encoded):UTF8", None),
        ],
        "check": lambda p: check_docx_md(p, [fixtures.MARK_DOCX]),
    },
    {
        "id": "docx2html",
        "label": "docx -> html",
        "src": "sample.docx",
        "out_suffix": ".html",
        "required": True,
        "attempts": [
            ("html:HTML (StarWriter)", "html:HTML (StarWriter)", None),
            ("html (bare)", "html", None),
        ],
        "check": lambda p: check_docx_html(p, [fixtures.MARK_DOCX, "caf\u00e9"]),
    },
    {
        "id": "csv2xlsx",
        "label": "csv -> xlsx",
        "src": "sample.csv",
        "out_suffix": ".xlsx",
        "required": True,
        "attempts": [
            ("infilter csv:44,34,76,1 + xlsx:Calc MS Excel 2007 XML",
             "xlsx:Calc MS Excel 2007 XML", CSV_INFILTER),
            ("xlsx (bare, no infilter)", "xlsx", None),
            ("quoted filter name (trap)", 'xlsx:"Calc MS Excel 2007 XML"', CSV_INFILTER),
        ],
        "check": check_csv_to_xlsx,
    },
    {
        "id": "xlsx2csv",
        "label": "xlsx -> csv",
        "src": "sample.xlsx",
        "out_suffix": ".csv",
        "required": True,
        "attempts": [
            ("csv:Text - txt - csv (StarCalc):44,34,76,1,,0,false,true,true,false,false,1",
             CSV_OUTFILTER, None),
            ("csv (bare, no options)", "csv", None),
            ("csv:44,34,76,1 (short token + options)", "csv:44,34,76,1", None),
            ("quoted filter name (trap)", 'csv:"Text - txt - csv (StarCalc)"', None),
        ],
        "check": lambda p: check_xlsx_to_csv(p, extra_sheet_marker=fixtures.MARK_XLSX_S2),
    },
    {
        "id": "html2docx",
        "label": "html -> docx",
        "src": "sample.html",
        "out_suffix": ".docx",
        "required": True,
        "attempts": [
            ("infilter HTML (StarWriter) + docx:MS Word 2007 XML",
             "docx:MS Word 2007 XML", "HTML (StarWriter)"),
            ("docx (bare, no infilter)", "docx", None),
            ("quoted filter name (trap)", 'docx:"MS Word 2007 XML"', "HTML (StarWriter)"),
        ],
        "check": lambda p: check_html_docx(p, [fixtures.MARK_HTML]),
    },
    {
        "id": "pptx2pdf",
        "label": "pptx -> pdf",
        "src": "sample.pptx",
        "out_suffix": ".pdf",
        "required": True,
        "attempts": [
            ("pdf:impress_pdf_Export", "pdf:impress_pdf_Export", None),
            ("pdf (bare)", "pdf", None),
        ],
        "check": lambda p: check_pdf(p, [fixtures.MARK_PPTX_1, fixtures.MARK_PPTX_2], min_pages=2),
    },
]


def expected_output(outdir, src, suffix):
    return os.path.join(outdir, os.path.splitext(src)[0] + suffix)


def find_products(outdir, suffix):
    """LibreOffice may rename the output: the CSV export filter with a sheet
    option writes <stem>-<sheet>.csv instead of <stem>.csv. Never assume the
    name -- glob the outdir for the expected extension."""
    if not os.path.isdir(outdir):
        return []
    return sorted(os.path.join(outdir, f) for f in os.listdir(outdir)
                  if f.lower().endswith(suffix))


# ----------------------------------------------------------------------- controls
def run_controls(args, out, work, logs, kw, version):
    """Record the failure modes around the matrix (not pass/fail gates)."""
    ctrl = []
    fx = os.path.join(work, "fixtures")
    print("\n== controls ==")

    # C1: first run on a brand-new profile, wrapper vs soffice.bin
    for execname, tag in ((os.path.basename(args.soffice), "c1-wrapper"), ("soffice.bin", "c1-soffice-bin")):
        exe = args.soffice if execname == os.path.basename(args.soffice) else \
            os.path.join(os.path.dirname(args.soffice), "soffice.bin")
        if not os.path.isfile(exe):
            ctrl.append({"id": tag, "skipped": "no such binary: %s" % exe})
            continue
        prof = os.path.join(work, tag + "-profile")
        shutil.rmtree(prof, ignore_errors=True)
        os.makedirs(prof, exist_ok=True)
        indir = os.path.join(work, tag + "-in")
        outdir = os.path.join(work, tag + "-out")
        for p in (indir, outdir):
            shutil.rmtree(p, ignore_errors=True)
            os.makedirs(p)
        shutil.copy2(os.path.join(fx, "sample.docx"), os.path.join(indir, "sample.docx"))
        rec = invoke(exe, os.path.join(indir, "sample.docx"), outdir, "pdf:writer_pdf_Export",
                     None, prof, tag, logs, **kw)
        produced = [os.path.basename(p) for p in find_products(outdir, ".pdf")]
        rec.update({"id": tag, "exec": exe, "first_run_on_fresh_profile": True,
                    "output_files": produced})
        ctrl.append(rec)
        print("   %-16s rc=%-3d %6.0f ms output=%s" % (tag, rec["rc"], rec["wall_ms"], produced or "NONE"))

    # C2: xlsx -> csv option syntaxes
    variants = [
        "csv:Text - txt - csv (StarCalc):44,34,76,1,,0,false,true,true,false,false,1",
        "csv:Text - txt - csv (StarCalc):44,34,76",
        "csv:Text - txt - csv (StarCalc)",
        "csv:Text - txt - csv (StarCalc):44,34,76,1",
        "csv:44,34,76",
        "csv:44,34,76,1,,0,false,true,true",
        "csv",
        "Text - txt - csv (StarCalc):44,34,76,1,,0,false,true,true,false,false,1",
        "csv:Text - txt - csv (StarCalc):44,34,76,1,,0,false,true,true,false,false,-1",
    ]
    prof = os.path.join(work, "c2-profile")
    shutil.rmtree(prof, ignore_errors=True)
    os.makedirs(prof, exist_ok=True)
    for i, conv in enumerate(variants):
        indir = os.path.join(work, "c2-in%d" % i)
        outdir = os.path.join(work, "c2-out%d" % i)
        for p in (indir, outdir):
            shutil.rmtree(p, ignore_errors=True)
            os.makedirs(p)
        shutil.copy2(os.path.join(fx, "sample.xlsx"), os.path.join(indir, "sample.xlsx"))
        rec = invoke(args.soffice, os.path.join(indir, "sample.xlsx"), outdir, conv, None,
                     prof, "c2-%d" % i, logs, **kw)
        files = find_products(outdir, ".csv")
        rec.update({"id": "c2-xlsx2csv:%s" % conv, "convert_arg": conv,
                    "output_files": [os.path.basename(f) for f in files],
                    "output_bytes": [os.path.getsize(f) for f in files]})
        ctrl.append(rec)
        print("   c2 %-62s rc=%-3d files=%s" % (conv, rec["rc"], rec["output_files"] or "NONE"))

    # C3: missing outdir / missing input
    prof = os.path.join(work, "c3-profile")
    os.makedirs(prof, exist_ok=True)
    indir = os.path.join(work, "c3-in")
    shutil.rmtree(indir, ignore_errors=True)
    os.makedirs(indir)
    shutil.copy2(os.path.join(fx, "sample.docx"), os.path.join(indir, "sample.docx"))
    ghost = os.path.join(work, "c3-outdir-that-does-not-exist")
    rec = invoke(args.soffice, os.path.join(indir, "sample.docx"), ghost, "pdf:writer_pdf_Export",
                 None, prof, "c3-missing-outdir", logs, **kw)
    rec.update({"id": "c3-missing-outdir", "outdir_existed_after": os.path.isdir(ghost),
                "output_files": [os.path.basename(p) for p in find_products(ghost, ".pdf")]})
    ctrl.append(rec)
    print("   c3 missing outdir rc=%-3d dir_created=%s files=%s"
          % (rec["rc"], rec.get("outdir_existed_after"), rec["output_files"] or "NONE"))

    outdir = os.path.join(work, "c3-out")
    os.makedirs(outdir, exist_ok=True)
    rec = invoke(args.soffice, os.path.join(indir, "no-such-file.docx"), outdir,
                 "pdf:writer_pdf_Export", None, prof, "c3-missing-input", logs, **kw)
    rec.update({"id": "c3-missing-input", "output_files": []})
    ctrl.append(rec)
    print("   c3 missing input  rc=%-3d stderr=%r" % (rec["rc"], rec["stderr"][:200]))

    # C4: docx->pdf fidelity on a multi-page document with an image
    png = os.path.join(work, "fidelity.png")
    fid_docx = os.path.join(work, "fidelity.docx")
    fixtures.build_fidelity_docx(fid_docx, png)
    indir = os.path.join(work, "c4-in")
    outdir = os.path.join(work, "c4-out")
    for p in (indir, outdir):
        shutil.rmtree(p, ignore_errors=True)
        os.makedirs(p)
    shutil.copy2(fid_docx, os.path.join(indir, "fidelity.docx"))
    prof = os.path.join(work, "c4-profile")
    shutil.rmtree(prof, ignore_errors=True)
    os.makedirs(prof, exist_ok=True)
    rec = invoke(args.soffice, os.path.join(indir, "fidelity.docx"), outdir,
                 "pdf:writer_pdf_Export", None, prof, "c4-fidelity", logs, **kw)
    pdfs = find_products(outdir, ".pdf")
    rec["id"] = "c4-docx2pdf-fidelity"
    rec["output_files"] = [os.path.basename(p) for p in pdfs]
    if pdfs:
        from pypdf import PdfReader

        reader = PdfReader(pdfs[0])
        texts = [(p.extract_text() or "") for p in reader.pages]
        blob = "\n".join(texts)
        fonts, images = set(), 0
        for page in reader.pages:
            res = page.get("/Resources") or {}
            for f in (res.get("/Font") or {}).values():
                fo = f.get_object()
                desc = fo.get("/FontDescriptor")
                embedded = False
                if desc:
                    embedded = any(k in desc.get_object() for k in ("/FontFile", "/FontFile2", "/FontFile3"))
                else:
                    for df in (fo.get("/DescendantFonts") or []):
                        d2 = df.get_object().get("/FontDescriptor")
                        if d2 and any(k in d2.get_object() for k in ("/FontFile", "/FontFile2", "/FontFile3")):
                            embedded = True
                fonts.add("%s embedded=%s" % (fo.get("/BaseFont"), embedded))
            for x in (res.get("/XObject") or {}).values():
                if x.get_object().get("/Subtype") == "/Image":
                    images += 1
        rec["fidelity"] = {
            "bytes": os.path.getsize(pdfs[0]),
            "pages": len(reader.pages),
            "page_sizes_pt": [[round(float(p.mediabox.width), 1), round(float(p.mediabox.height), 1)]
                              for p in reader.pages],
            "markers": {m: (m in blob) for m in ("MARKER-PAGE1", "MARKER-PAGE2", "MARKER-PAGE-LAST")},
            "fonts": sorted(fonts),
            "image_xobjects": images,
            "text_chars": len(blob),
            "tail": blob[-100:].replace("\n", " | "),
        }
    ctrl.append(rec)
    print("   c4 fidelity rc=%-3d %s" % (rec["rc"], json.dumps(rec.get("fidelity", {}))[:300]))

    with open(os.path.join(out, "controls.json"), "w", encoding="utf-8") as fh:
        json.dump({"version": version, "controls": ctrl}, fh, indent=2)
    print("   -> controls.json (%d records)" % len(ctrl))
    return ctrl


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--soffice", default=os.environ.get("SOFFICE") or shutil.which("soffice") or "soffice")
    ap.add_argument("--out", default=os.path.join(HERE, "out"))
    ap.add_argument("--reps", type=int, default=3)
    ap.add_argument("--ld-library-path", default=os.environ.get("S3_LD_LIBRARY_PATH", ""))
    ap.add_argument("--controls", action="store_true", help="also run the no-output trap controls")
    args = ap.parse_args()

    out = os.path.abspath(args.out)
    work = os.path.join(out, "work")
    logs = os.path.join(out, "logs")
    products = os.path.join(out, "products")
    fx = os.path.join(work, "fixtures")
    home = os.path.join(work, "home")
    for d in (out, work, logs, products, home):
        os.makedirs(d, exist_ok=True)
    kw = {"ld_library_path": args.ld_library_path or None, "home": home}

    print("== S3 document direction matrix ==")
    print("soffice : %s" % args.soffice)
    fixtures.build(fx)

    v_env = dict(os.environ)
    v_env["HOME"] = home
    if args.ld_library_path:
        v_env["LD_LIBRARY_PATH"] = args.ld_library_path
    ver = subprocess.run([args.soffice, "--headless", "--version"], capture_output=True,
                         text=True, timeout=240, env=v_env)
    raw_ver = ((ver.stdout or "") + (ver.stderr or "")).strip()
    version = raw_ver.splitlines()[0] if raw_ver else "unknown (rc=%d)" % ver.returncode
    print("version : %s" % version)
    print("host    : %s %s, %s cores" % (platform.system(), platform.release(), os.cpu_count()))

    results = []
    for d in DIRECTIONS:
        print("\n-- %s --" % d["label"])
        profile = os.path.join(work, "profile-" + d["id"])
        winner = None
        attempts_log = []
        shutil.rmtree(profile, ignore_errors=True)
        os.makedirs(profile, exist_ok=True)
        for idx, (label, convert_arg, infilter) in enumerate(d["attempts"]):
            indir = os.path.join(work, d["id"], "attempt%d-in" % idx)
            outdir = os.path.join(work, d["id"], "attempt%d-out" % idx)
            for p in (indir, outdir):
                shutil.rmtree(p, ignore_errors=True)
                os.makedirs(p)
            src = os.path.join(indir, d["src"])
            shutil.copy2(os.path.join(fx, d["src"]), src)
            rec = invoke(args.soffice, src, outdir, convert_arg, infilter, profile,
                         "%s-attempt%d" % (d["id"], idx), logs, **kw)
            target = expected_output(outdir, d["src"], d["out_suffix"])
            produced = find_products(outdir, d["out_suffix"])
            if produced:
                target = produced[0]
            rec["attempt"] = label
            rec["convert_arg"] = convert_arg
            rec["infilter"] = infilter
            rec["output_path"] = target
            rec["output_files"] = [os.path.basename(p) for p in produced]
            rec["output_exists"] = bool(produced)
            rec["output_bytes"] = os.path.getsize(target) if produced else 0
            line = {"attempt": label, "rc": rec["rc"], "exists": rec["output_exists"],
                    "bytes": rec["output_bytes"], "wall_ms": rec["wall_ms"]}
            print("   attempt %d [%s] rc=%s exists=%s bytes=%d %s"
                  % (idx, label, rec["rc"], rec["output_exists"], rec["output_bytes"], human(rec["wall_ms"])))
            if rec["output_exists"] and rec["output_bytes"] > 0:
                ok, details, metrics = d["check"](target)
                rec["content_ok"] = ok
                rec["content_details"] = details
                rec["metrics"] = metrics
                line["content_ok"] = ok
                print("      content: %s" % ("PASS" if ok else "FAIL"))
                for det in details:
                    print("        " + det)
                if ok:
                    rec["product"] = os.path.join(products, d["id"] + d["out_suffix"])
                    shutil.copy2(target, rec["product"])
                    attempts_log.append(line)
                    winner = rec
                    break
            else:
                rec["content_ok"] = None
                line["content_ok"] = None
            if not (rec["stdout"] or rec["stderr"]):
                line["note"] = "no stdout/stderr"
            line["stderr_tail"] = (rec["stderr"] or rec["stdout"])[-400:]
            attempts_log.append(line)

        entry = {
            "id": d["id"],
            "label": d["label"],
            "required": d["required"],
            "attempts": attempts_log,
        }
        if winner is None:
            entry["verdict"] = "FAIL"
            results.append(entry)
            print("   verdict: FAIL (no attempt produced a valid file)")
            continue

        # ------------------------------------------ cold vs warm timing (winner)
        cold_profile = os.path.join(work, "cold-" + d["id"])
        shutil.rmtree(cold_profile, ignore_errors=True)
        os.makedirs(cold_profile, exist_ok=True)
        cdir_in = os.path.join(work, d["id"], "cold-in")
        cdir_out = os.path.join(work, d["id"], "cold-out")
        for p in (cdir_in, cdir_out):
            shutil.rmtree(p, ignore_errors=True)
            os.makedirs(p)
        src = os.path.join(cdir_in, d["src"])
        shutil.copy2(os.path.join(fx, d["src"]), src)
        cold = invoke(args.soffice, src, cdir_out, winner["convert_arg"], winner["infilter"],
                      cold_profile, d["id"] + "-cold", logs, **kw)
        cold_target = expected_output(cdir_out, d["src"], d["out_suffix"])
        cold_files = find_products(cdir_out, d["out_suffix"])
        if cold_files:
            cold_target = cold_files[0]
        cold_exists = bool(cold_files) and os.path.getsize(cold_target) > 0
        cold_check = d["check"](cold_target) if cold_exists else (False, [], {})
        print("   cold : %s rc=%s exists=%s content=%s"
              % (human(cold["wall_ms"]), cold["rc"], cold_exists, cold_check[0]))

        warm_ms = []
        for rep in range(args.reps):
            wdir_in = os.path.join(work, d["id"], "warm-in")
            wdir_out = os.path.join(work, d["id"], "warm-out")
            shutil.rmtree(wdir_out, ignore_errors=True)
            shutil.rmtree(wdir_in, ignore_errors=True)
            os.makedirs(wdir_in)
            os.makedirs(wdir_out)
            shutil.copy2(os.path.join(fx, d["src"]), os.path.join(wdir_in, d["src"]))
            w = invoke(args.soffice, os.path.join(wdir_in, d["src"]), wdir_out,
                       winner["convert_arg"], winner["infilter"], cold_profile,
                       "%s-warm%d" % (d["id"], rep + 1), logs, **kw)
            target = expected_output(wdir_out, d["src"], d["out_suffix"])
            w_files = find_products(wdir_out, d["out_suffix"])
            if w_files:
                target = w_files[0]
            ok = bool(w_files) and os.path.getsize(target) > 0
            print("   warm%d: %s rc=%s exists=%s" % (rep + 1, human(w["wall_ms"]), w["rc"], ok))
            if ok:
                warm_ms.append(w["wall_ms"])

        entry.update({
            "verdict": "PASS" if (cold_exists and cold_check[0] and len(warm_ms) == args.reps) else "FAIL",
            "winning_attempt": winner["attempt"],
            "convert_arg": winner["convert_arg"],
            "infilter": winner["infilter"],
            "argv": winner["argv"],
            "rc": winner["rc"],
            "output_path": winner["product"],
            "output_files": winner.get("output_files", []),
            "output_bytes": os.path.getsize(winner["product"]),
            "output_sha256": sha256(winner["product"]),
            "content_ok": winner["content_ok"],
            "content_details": winner["content_details"],
            "metrics": winner["metrics"],
            "cold_ms": cold["wall_ms"],
            "cold_rc": cold["rc"],
            "cold_content_ok": cold_check[0],
            "warm_ms": warm_ms,
            "warm_median_ms": round(statistics.median(warm_ms), 1) if warm_ms else None,
        })
        print("   verdict: %s (cold %s, warm median %s)"
              % (entry["verdict"], human(cold["wall_ms"]),
                 human(statistics.median(warm_ms)) if warm_ms else "n/a"))
        results.append(entry)

    controls = None
    if args.controls:
        controls = run_controls(args, out, work, logs, kw, version)

    payload = {
        "spike": "s3",
        "soffice": args.soffice,
        "soffice_version": version,
        "host": {
            "system": platform.system(),
            "release": platform.release(),
            "machine": platform.machine(),
            "cpus": os.cpu_count(),
            "python": platform.python_version(),
        },
        "directions": results,
        "verdict": "PASS" if all(r["verdict"] == "PASS" or not r["required"] for r in results) else "FAIL",
    }
    with open(os.path.join(out, "result.json"), "w", encoding="utf-8") as fh:
        json.dump(payload, fh, indent=2)

    lines = [
        "# S3 document direction matrix",
        "",
        "soffice: `%s`  " % version,
        "host: %s %s, %s cores" % (platform.system(), platform.release(), os.cpu_count()),
        "",
        "| direction | verdict | invocation | output | bytes | cold | warm median | content check |",
        "|---|---|---|---|---|---|---|---|",
    ]
    for r in results:
        if r["verdict"] == "PASS":
            inv = "--convert-to %s%s" % (
                r["convert_arg"], (" --infilter=%s" % r["infilter"]) if r.get("infilter") else "")
            lines.append("| %s | PASS | `%s` | `%s` | %d | %s | %s | ok |" % (
                r["label"], inv,
                (r.get("output_files") or [os.path.basename(r["output_path"])])[0],
                r["output_bytes"], human(r["cold_ms"]), human(r["warm_median_ms"])))
        else:
            tried = "; ".join(a["attempt"] for a in r["attempts"])
            lines.append("| %s | FAIL | tried: %s | - | 0 | - | - | no output or content mismatch |" % (
                r["label"], tried))
    with open(os.path.join(out, "table.md"), "w", encoding="utf-8") as fh:
        fh.write("\n".join(lines) + "\n")

    print("\n== summary ==")
    print("\n".join(lines))
    print("\nverdict: %s" % payload["verdict"])
    return 0 if payload["verdict"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
