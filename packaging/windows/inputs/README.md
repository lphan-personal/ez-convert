# Windows engine-pack inputs

`vips-8.18.7-spec-aom-dist.tar.gz` — the libvips sidecar directory that the S2/S2b
spikes built and measured, byte-for-byte. It is the only engine input that cannot be
fetched from upstream: there is no public GPL-free Windows libvips build (the stock
`vips-dev-x64-web` bundles link GPL `libimagequant` and cannot decode HEIC), so the
tree we ship is the one built and audited on the spike host and recorded in
`evidence/s2-libvips.md` and `spikes/s4/artifacts/FINAL-dist-spec-aom-sha256.txt`.

| property | value |
|---|---|
| files | 43 (35,824,560 B uncompressed) |
| tarball sha256 | `0dff47413690bf9ae3d0599e5fb4a04ff03419dcb191af80611fbcb73b2b9dc9` |
| per-file digests | `docs/shipped-inventory.tsv`, rows `vips dist (spec-aom final)` |
| built from | libvips 8.18.7, libheif 1.23.7 (decoders dav1d/de265, encoder libaom), libtiff 4.7.2 with `-Djbig=OFF`; MSYS2/UCRT64 gcc 16.2.0 |
| build flags | `-Dimagequant=disabled -Dquantizr=disabled -Dpoppler=disabled -Dpdfium=disabled -Dmagick=disabled -Dopenjpeg=disabled -Dheif=enabled -Dmodules=disabled` |

`scripts/win-engine-pack.sh` pins the tarball digest and refuses to build from anything
else; `scripts/dist-license-gate.sh` then re-checks all 43 per-file digests against the
inventory, so a swapped or rebuilt DLL cannot slip into the pack.

## Regenerating this tarball

From a Windows box with MSYS2/UCRT64 and the S2 recipe
(`build-vips-gplfree-windows.sh`, `VARIANT=spec-aom`, documented in
`evidence/s2-libvips.md` §"Reproduce"), after the dist has been built:

```bash
# the dist directory the recipe leaves at C:\build\dist
tar --sort=name --owner=0 --group=0 --numeric-owner \
    --mtime='2026-11-01 12:00:00Z' \
    -czf vips-8.18.7-spec-aom-dist.tar.gz -C /c/build/dist .
sha256sum vips-8.18.7-spec-aom-dist.tar.gz
```

A rebuild is only usable if **all 43 per-file digests still match the inventory rows** —
a different MSYS2 package version would change them, which is an inventory change and
a deliberate decision, not a rebuild detail.

## Other inputs (fetched in CI, pinned in the script)

| input | pinned by |
|---|---|
| `ffmpeg-N-128273-g836436cfb9-win64-lgpl.zip` (BtbN FFmpeg-Builds, dated tag `autobuild-2026-11-01-12-49`) | zip sha256 `00bfdb3e…`, `ffmpeg.exe` `da90280e…`, `ffprobe.exe` `894f68a5…` — the two exe digests are the inventory rows |
| `LibreOffice_26.8.2_Win_x86-64.msi` (TDF) | msi sha256 `ff4b9114…`, measured by S3 on 2026-11-01 |
