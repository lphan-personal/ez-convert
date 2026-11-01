// pdfjs_text.mjs — read the PDF text layer through pdf.js, the engine Firefox uses for the
// text layer behind selection and Ctrl+F. Same question as the browsers: is U+00E9 there?
import { getDocument } from 'pdfjs-dist/legacy/build/pdf.mjs';
import fs from 'fs';

const path = process.argv[2];
const data = new Uint8Array(fs.readFileSync(path));
const doc = await getDocument({ data, useSystemFonts: true }).promise;
let all = '';
for (let i = 1; i <= doc.numPages; i++) {
  const page = await doc.getPage(i);
  const tc = await page.getTextContent();
  all += tc.items.map(it => it.str).join(' ') + '\n';
}
fs.writeFileSync(process.argv[3], all, 'utf8');
const has = s => all.includes(s);
console.log('pdfjs version :', (await import('pdfjs-dist/package.json', { with: { type: 'json' } })).default.version);
console.log('pages         :', doc.numPages);
console.log('chars         :', all.length);
console.log('cafe accent   :', has('caf\u00e9'));
console.log('cafe + 0x1c   :', has('cafe\u001c'));
console.log('naive ok      :', has('na\u00efve'));
console.log('naive + 0x1f  :', has('na\u00ef\u001fve'));
console.log('cjk           :', has('\u4e2d\u6587'));
console.log('MARKER-ALPHA  :', has('MARKER-ALPHA'));
const i = all.indexOf('MARKER');
const seg = all.slice(Math.max(0, i - 10), i + 120);
console.log('excerpt       :', JSON.stringify(seg));
