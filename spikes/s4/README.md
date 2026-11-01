# S4 — engine-pack verification

| file | what |
|---|---|
| `EVIDENCE.md` | the two verdicts, every number, and what is *not* measured |
| `artifacts/` | raw tool output: avif suite log + dist manifest, docx→pdf run, pypdf and pdf.js text-layer dumps, viewer clipboard/UIA dumps and screenshots |
| `harness/` | what was actually run on the Windows box |

`harness/` is Windows-only and was **not** shipped; it exists so the run can be repeated:

- `mkmanifest.sh` — sha256 manifest of a vips dist (MSYS2 bash).
- `convert.ps1` — the S3-proven `soffice.com` argv, fresh `-env:UserInstallation` profile.
- `inspect_text.py` — the `pypdf` control (extraction + per-font `ToUnicode` `bfchar` codes).
- `pdfjs_text.mjs` — `pdf.js` text layer via Node (`npm i pdfjs-dist@5`).
- `screen.ps1` — proves whether the session can see a desktop at all (`OpenInputDesktop`).
- `launcher.ps1` — SYSTEM scheduled task → `WTSQueryUserToken` + `CreateProcessAsUser` into the
  logged-on console session, because SSH lands in session 0 where no window exists.
- `viewer_test2.ps1` / `viewer_test5.ps1` — drive Chrome/Edge/Brave: screenshot, find-bar result
  out of UI Automation, clipboard read via Win32 `CF_UNICODETEXT`.
- `push.sh` / `showclip.py` — base64 transfer (scp is unreliable on that host) and
  control-character-safe display of what landed on the clipboard.
