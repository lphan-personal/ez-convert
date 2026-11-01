#!/usr/bin/env python3
"""S3 fixture corpus — deterministic inputs for the LibreOffice direction matrix.

Every fixture carries marker strings that the *output* must contain, so a
conversion can be checked for content and not just for existence.

    python3 fixtures.py <outdir>

Requires: python-docx, openpyxl, python-pptx  (verification-only deps; the
app itself never uses them).
"""
import os
import sys

MARK_DOCX = "MARKER-ALPHA"
MARK_HTML = "MARKER-BETA"
MARK_XLSX_S2 = "MARKER-SHEET2"
MARK_PPTX_1 = "MARKER-GAMMA"
MARK_PPTX_2 = "MARKER-DELTA"
UNICODE_SAMPLE = "caf\u00e9 \u2013 na\u00efve \u2192 \u4e2d\u6587"
CSV_ROWS = [
    ["id", "name", "amount", "note"],
    ["1", "Widget A", "12.50", "comma, inside"],
    ["2", "Gadget B", "-3.75", "caf\u00e9"],
    ["3", "Thing C", "1000", "plain"],
]


def make_docx(path):
    from docx import Document
    from docx.shared import Pt

    doc = Document()
    doc.core_properties.title = "EZ Convert S3 fixture"
    doc.add_heading("EZ Convert S3 fixture", level=1)
    doc.add_paragraph(
        "%s this paragraph proves text survives the conversion. %s"
        % (MARK_DOCX, UNICODE_SAMPLE)
    )
    p = doc.add_paragraph("Second paragraph with a tab\tbetween\tvalues and 100%% literal.")
    p.runs[0].font.size = Pt(11)
    doc.add_paragraph("Third paragraph: bullet-like line one.\nLine two after a soft break.")
    table = doc.add_table(rows=3, cols=3)
    table.style = "Table Grid"
    cells = [["Metric", "Value", "Unit"],
             ["Pages", "1", "page"],
             ["Marker", MARK_DOCX, "string"]]
    for r, row in enumerate(cells):
        for c, val in enumerate(row):
            table.cell(r, c).text = val
    doc.add_paragraph("Closing paragraph after the table.")
    doc.save(path)


def make_xlsx(path):
    from openpyxl import Workbook

    wb = Workbook()
    ws = wb.active
    ws.title = "Data"
    for r, row in enumerate(CSV_ROWS, start=1):
        for c, val in enumerate(row, start=1):
            if r > 1 and c == 1:
                ws.cell(r, c, int(val))
            elif r > 1 and c == 3:
                ws.cell(r, c, float(val))
            else:
                ws.cell(r, c, val)
    ws["F1"] = "total"
    ws["F2"] = "=SUM(C2:C4)"
    ws["G1"] = UNICODE_SAMPLE
    ws2 = wb.create_sheet("Notes")
    ws2["A1"] = MARK_XLSX_S2
    ws2["A2"] = 42
    wb.save(path)


def make_pptx(path):
    from pptx import Presentation
    from pptx.util import Inches

    prs = Presentation()
    s1 = prs.slides.add_slide(prs.slide_layouts[1])
    s1.shapes.title.text = "S3 pptx fixture"
    s1.placeholders[1].text = "%s first slide body: %s" % (MARK_PPTX_1, UNICODE_SAMPLE)
    tb = s1.shapes.add_textbox(Inches(1), Inches(5), Inches(6), Inches(1))
    tb.text_frame.text = "Second line on slide one."
    s2 = prs.slides.add_slide(prs.slide_layouts[1])
    s2.shapes.title.text = "Second slide"
    s2.placeholders[1].text = "%s second slide body." % MARK_PPTX_2
    prs.save(path)


CSV_TEXT = (
    "id,name,amount,note\r\n"
    "1,Widget A,12.50,\"comma, inside\"\r\n"
    "2,Gadget B,-3.75,caf\u00e9\r\n"
    "3,Thing C,1000,plain\r\n"
)

HTML_TEXT = """<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>S3 html fixture</title>
</head>
<body>
<h1>%s heading</h1>
<p>First paragraph carrying the marker and unicode: %s</p>
<p>Second paragraph with an entity: AT&amp;T and a number 42.</p>
<table border="1">
  <tr><th>Column</th><th>Value</th></tr>
  <tr><td>Marker</td><td>%s</td></tr>
</table>
<ul><li>list item one</li><li>list item two</li></ul>
</body>
</html>
""" % (MARK_HTML, UNICODE_SAMPLE, MARK_HTML)


def tiny_png(path, w=64, h=64, rgb=(200, 30, 30)):
    """A real PNG, built with the stdlib so no image library is needed."""
    import struct
    import zlib

    raw = b"".join(b"\x00" + bytes(rgb) * w for _ in range(h))

    def chunk(tag, data):
        return (struct.pack(">I", len(data)) + tag + data
                + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF))

    png = (b"\x89PNG\r\n\x1a\n"
           + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
           + chunk(b"IDAT", zlib.compress(raw, 9))
           + chunk(b"IEND", b""))
    with open(path, "wb") as fh:
        fh.write(png)


def build_fidelity_docx(path, png_path):
    """A multi-page docx with an image, for the docx->pdf fidelity probe."""
    from docx import Document
    from docx.shared import Inches

    tiny_png(png_path)
    doc = Document()
    doc.add_heading("Fidelity fixture", level=1)
    doc.add_paragraph("MARKER-PAGE1 first page text.")
    doc.add_picture(png_path, width=Inches(2.0))
    for i in range(2, 40):
        doc.add_paragraph("Fill line %02d: the quick brown fox jumps over the lazy dog." % i)
        if i in (18, 19):
            doc.add_paragraph("MARKER-PAGE2 mid document marker at fill line %02d." % i)
    for i in range(40, 80):
        doc.add_paragraph("Tail line %02d." % i)
    doc.add_paragraph("MARKER-PAGE-LAST closing marker.")
    doc.save(path)


def build(outdir):
    os.makedirs(outdir, exist_ok=True)
    make_docx(os.path.join(outdir, "sample.docx"))
    make_xlsx(os.path.join(outdir, "sample.xlsx"))
    make_pptx(os.path.join(outdir, "sample.pptx"))
    with open(os.path.join(outdir, "sample.csv"), "w", encoding="utf-8", newline="") as fh:
        fh.write(CSV_TEXT)
    with open(os.path.join(outdir, "sample.html"), "w", encoding="utf-8", newline="") as fh:
        fh.write(HTML_TEXT)
    for name in sorted(os.listdir(outdir)):
        p = os.path.join(outdir, name)
        print("fixture %-14s %8d B" % (name, os.path.getsize(p)))


if __name__ == "__main__":
    build(sys.argv[1] if len(sys.argv) > 1 else "fixtures")
