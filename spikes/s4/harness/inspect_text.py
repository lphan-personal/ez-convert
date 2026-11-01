# inspect_text.py — control: reproduce the S3 text-layer finding on THIS pdf, before any viewer test.
import sys, pypdf, json

path = sys.argv[1] if len(sys.argv) > 1 else r'C:\build\s4\pdf\out\sample.pdf'
r = pypdf.PdfReader(path)
out = {'path': path, 'pdf_header': open(path, 'rb').read(8).decode('latin-1'),
       'pages': len(r.pages), 'pypdf_version': pypdf.__version__}
p = r.pages[0]
txt = p.extract_text()
out['pypdf_text_repr'] = repr(txt[:400])
out['contains_cafe_accent'] = 'café' in txt
out['contains_cafe_0x1c'] = 'cafe\x1c' in txt
out['contains_naive_0x1f'] = 'na\xefve' not in txt
out['ctrl_codes'] = sorted(set(c for c in txt if ord(c) < 32 and c not in '\r\n\t'))

# fonts on page 1
res = p.get('/Resources', {})
fonts = res.get('/Font', {})
finfo = {}
for name, ref in fonts.items():
    f = ref.get_object()
    entry = {'Subtype': f.get('/Subtype'), 'BaseFont': str(f.get('/BaseFont')),
             'Encoding': str(f.get('/Encoding'))}
    if '/ToUnicode' in f:
        data = f['/ToUnicode'].get_object().get_data().decode('latin-1', 'replace')
        entry['tounicode_len'] = len(data)
        entry['tounicode_has_1C'] = '001c' in data.lower() or '<1c>' in data.lower()
        entry['tounicode_has_1F'] = '001f' in data.lower() or '<1f>' in data.lower()
        import re
        codes = re.findall(r'<([0-9a-fA-F]{2})>', data)
        entry['bfchar_codes'] = codes
    finfo[str(name)] = entry
out['fonts_page1'] = finfo
json.dump(out, open(r'C:\build\s4\pdf\pypdf-control.json', 'w'), indent=2)
print(json.dumps(out, indent=2))
