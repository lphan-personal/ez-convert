import sys, unicodedata

def show(path, needle='MARKER'):
    raw = open(path, 'rb').read()
    try:
        d = raw.decode('utf-8')
    except UnicodeDecodeError:
        d = raw.decode('utf-8', errors='replace')
    print('=' * 20, path, 'len=', len(d))
    i = d.find(needle)
    seg = d[max(0, i - 10):i + 170] if i >= 0 else d[:300]
    out = []
    for ch in seg:
        o = ord(ch)
        if 32 <= o <= 126:
            out.append(ch)
        else:
            out.append('[%04X]' % o)
    print(''.join(out))
    print('bytes around 0x1c/0x1f present:', '\x1c' in d, '\x1f' in d)
    print('cafe-accent:', ('caf' + chr(0xE9)) in d, ' naive:', ('na' + chr(0xEF) + 've') in d, ' cjk:', chr(0x4E2D) in d)

for p in sys.argv[1:]:
    show(p)
