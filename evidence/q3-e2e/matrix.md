| direction | family | verdict | metadata policy | detail |
|---|---|---|---|---|
| `precondition` | policy | PASS | keep=default | Options::default().strip_metadata == false (SPEC §5 keep-by-default) |
| `no duplicate directions` | preflight | PASS | — | 106 advertised media directions are unique |
| `fixture per input type` | preflight | PASS | — | 28 input types resolve to a fixture file |
| `every advertised direction is routed` | preflight | PASS | — | 0 unrouted of 106 advertised |
| `document sets agree` | preflight | PASS | — | 42 directions in both matrix and engine::soffice::proven_routes |
| `evidence still says so` | preflight | PASS | — | every advertised route's evidence file contains its marker literal |
| `jpeg->jpg` | image | PASS | icc=kept strip=clean | 64x48 mean=124.8 2725B |
| `jpeg->png` | image | PASS | icc=kept strip=clean | 64x48 mean=124.8 3537B |
| `jpeg->webp` | image | PASS | icc=kept strip=clean | 64x48 mean=125.0 1188B |
| `jpeg->tiff` | image | PASS | icc=kept strip=clean | 64x48 mean=124.8 3508B |
| `jpeg->avif` | image | SKIP | — | engine cannot do this on this dist (heif/av1 unavailable): engine failed: step 1 (vips heifsave (av1)): engine exited 1; stderr: heifsave: Unsupported compression |
| `jpeg->gif` | image | PASS | icc=n/a strip=clean | 64x48 mean=124.8 2904B |
| `png->jpg` | image | PASS | icc=kept strip=clean | 64x48 mean=124.9 2781B |
| `png->webp` | image | PASS | icc=kept strip=clean | 64x48 mean=125.2 1130B |
| `png->tiff` | image | PASS | icc=kept strip=clean | 64x48 mean=124.9 522B |
| `png->avif` | image | SKIP | — | engine cannot do this on this dist (heif/av1 unavailable): engine failed: step 1 (vips heifsave (av1)): engine exited 1; stderr: heifsave: Unsupported compression |
| `png->gif` | image | PASS | icc=n/a strip=clean | 64x48 mean=125.1 1680B |
| `heic->jpg` | image | PASS | n/a: no heic fixture carrying EXIF/ICC can be produced on this dist | 200x400 mean=111.7 5747B |
| `heic->png` | image | PASS | n/a: no heic fixture carrying EXIF/ICC can be produced on this dist | 200x400 mean=111.8 1940B |
| `heic->webp` | image | PASS | n/a: no heic fixture carrying EXIF/ICC can be produced on this dist | 200x400 mean=111.9 740B |
| `heic->tiff` | image | PASS | n/a: no heic fixture carrying EXIF/ICC can be produced on this dist | 200x400 mean=111.8 2788B |
| `heic->avif` | image | SKIP | — | engine cannot do this on this dist (heif/av1 unavailable): engine failed: step 1 (vips heifsave (av1)): engine exited 1; stderr: heifsave: Unsupported compression |
| `heic->gif` | image | PASS | n/a: no heic fixture carrying EXIF/ICC can be produced on this dist | 200x400 mean=111.4 7756B |
| `avif->jpg` | image | PASS | n/a: no avif fixture carrying EXIF/ICC can be produced on this dist | 64x48 mean=125.1 2594B |
| `avif->png` | image | PASS | n/a: no avif fixture carrying EXIF/ICC can be produced on this dist | 64x48 mean=125.1 2350B |
| `avif->webp` | image | PASS | n/a: no avif fixture carrying EXIF/ICC can be produced on this dist | 64x48 mean=125.3 1162B |
| `avif->tiff` | image | PASS | n/a: no avif fixture carrying EXIF/ICC can be produced on this dist | 64x48 mean=125.1 2412B |
| `avif->gif` | image | PASS | n/a: no avif fixture carrying EXIF/ICC can be produced on this dist | 64x48 mean=125.3 2454B |
| `webp->jpg` | image | PASS | icc=kept strip=clean | 64x48 mean=125.2 2904B |
| `webp->png` | image | PASS | icc=kept strip=clean | 64x48 mean=125.2 4346B |
| `webp->tiff` | image | PASS | icc=kept strip=clean | 64x48 mean=125.2 4268B |
| `webp->avif` | image | SKIP | — | engine cannot do this on this dist (heif/av1 unavailable): engine failed: step 1 (vips heifsave (av1)): engine exited 1; stderr: heifsave: Unsupported compression |
| `webp->gif` | image | PASS | icc=n/a strip=clean | 64x48 mean=125.6 2812B |
| `gif->jpg` | image | PASS | n/a: no gif fixture carrying EXIF/ICC can be produced on this dist | 64x48 mean=124.4 3010B |
| `gif->png` | image | PASS | n/a: no gif fixture carrying EXIF/ICC can be produced on this dist | 64x48 mean=124.3 770B |
| `gif->webp` | image | PASS | n/a: no gif fixture carrying EXIF/ICC can be produced on this dist | 64x48 mean=124.6 1332B |
| `gif->tiff` | image | PASS | n/a: no gif fixture carrying EXIF/ICC can be produced on this dist | 64x48 mean=124.3 774B |
| `gif->avif` | image | SKIP | — | engine cannot do this on this dist (heif/av1 unavailable): engine failed: step 1 (vips heifsave (av1)): engine exited 1; stderr: heifsave: Unsupported compression |
| `tiff->jpg` | image | PASS | icc=kept strip=clean | 64x48 mean=124.9 2781B |
| `tiff->png` | image | PASS | icc=kept strip=clean | 64x48 mean=124.9 626B |
| `tiff->webp` | image | PASS | icc=kept strip=clean | 64x48 mean=125.2 1130B |
| `tiff->avif` | image | SKIP | — | engine cannot do this on this dist (heif/av1 unavailable): engine failed: step 1 (vips heifsave (av1)): engine exited 1; stderr: heifsave: Unsupported compression |
| `tiff->gif` | image | PASS | icc=n/a strip=clean | 64x48 mean=125.1 1680B |
| `mp3->mp3` | audio | PASS | tags=carried strip=clean | mp3 44100 Hz 1 ch 250 ms |
| `mp3->wav` | audio | PASS | tags=carried strip=clean | pcm_s16le 44100 Hz 1 ch 250 ms |
| `mp3->flac` | audio | PASS | tags=carried strip=clean | flac 44100 Hz 1 ch 250 ms |
| `mp3->m4a` | audio | PASS | tags=carried strip=clean | aac 44100 Hz 1 ch 250 ms |
| `mp3->aac` | audio | PASS | tags=carried strip=clean | aac 44100 Hz 1 ch 279 ms |
| `mp3->ogg` | audio | PASS | tags=carried strip=clean | vorbis 44100 Hz 1 ch 250 ms |
| `mp3->opus` | audio | PASS | tags=carried strip=clean | opus 48000 Hz 1 ch 257 ms |
| `wav->mp3` | audio | PASS | tags=carried strip=clean | mp3 44100 Hz 1 ch 250 ms |
| `wav->wav` | audio | PASS | tags=carried strip=clean | pcm_s16le 44100 Hz 1 ch 250 ms |
| `wav->flac` | audio | PASS | tags=carried strip=clean | flac 44100 Hz 1 ch 250 ms |
| `wav->m4a` | audio | PASS | tags=carried strip=clean | aac 44100 Hz 1 ch 250 ms |
| `wav->aac` | audio | PASS | tags=carried strip=clean | aac 44100 Hz 1 ch 279 ms |
| `wav->ogg` | audio | PASS | tags=carried strip=clean | vorbis 44100 Hz 1 ch 250 ms |
| `wav->opus` | audio | PASS | tags=carried strip=clean | opus 48000 Hz 1 ch 257 ms |
| `flac->mp3` | audio | PASS | tags=carried strip=clean | mp3 44100 Hz 1 ch 250 ms |
| `flac->wav` | audio | PASS | tags=carried strip=clean | pcm_s16le 44100 Hz 1 ch 250 ms |
| `flac->flac` | audio | PASS | tags=carried strip=clean | flac 44100 Hz 1 ch 250 ms |
| `flac->m4a` | audio | PASS | tags=carried strip=clean | aac 44100 Hz 1 ch 250 ms |
| `flac->aac` | audio | PASS | tags=carried strip=clean | aac 44100 Hz 1 ch 279 ms |
| `flac->ogg` | audio | PASS | tags=carried strip=clean | vorbis 44100 Hz 1 ch 250 ms |
| `flac->opus` | audio | PASS | tags=carried strip=clean | opus 48000 Hz 1 ch 257 ms |
| `m4a->mp3` | audio | PASS | tags=carried strip=clean | mp3 44100 Hz 1 ch 250 ms |
| `m4a->wav` | audio | PASS | tags=carried strip=clean | pcm_s16le 44100 Hz 1 ch 250 ms |
| `m4a->flac` | audio | PASS | tags=carried strip=clean | flac 44100 Hz 1 ch 250 ms |
| `m4a->m4a` | audio | PASS | tags=carried strip=clean | aac 44100 Hz 1 ch 250 ms |
| `m4a->aac` | audio | PASS | tags=carried strip=clean | aac 44100 Hz 1 ch 279 ms |
| `m4a->ogg` | audio | PASS | tags=carried strip=clean | vorbis 44100 Hz 1 ch 250 ms |
| `m4a->opus` | audio | PASS | tags=carried strip=clean | opus 48000 Hz 1 ch 257 ms |
| `aac->mp3` | audio | PASS | tags=carried strip=clean | mp3 44100 Hz 1 ch 279 ms |
| `aac->wav` | audio | PASS | tags=carried strip=clean | pcm_s16le 44100 Hz 1 ch 279 ms |
| `aac->flac` | audio | PASS | tags=carried strip=clean | flac 44100 Hz 1 ch 279 ms |
| `aac->m4a` | audio | PASS | tags=carried strip=clean | aac 44100 Hz 1 ch 279 ms |
| `aac->aac` | audio | PASS | tags=carried strip=clean | aac 44100 Hz 1 ch 302 ms |
| `aac->ogg` | audio | PASS | tags=carried strip=clean | vorbis 44100 Hz 1 ch 279 ms |
| `aac->opus` | audio | PASS | tags=carried strip=clean | opus 48000 Hz 1 ch 285 ms |
| `ogg->mp3` | audio | PASS | tags=carried strip=clean | mp3 44100 Hz 1 ch 247 ms |
| `ogg->wav` | audio | PASS | tags=carried strip=clean | pcm_s16le 44100 Hz 1 ch 247 ms |
| `ogg->flac` | audio | PASS | tags=carried strip=clean | flac 44100 Hz 1 ch 247 ms |
| `ogg->m4a` | audio | PASS | tags=carried strip=clean | aac 44100 Hz 1 ch 247 ms |
| `ogg->aac` | audio | PASS | tags=carried strip=clean | aac 44100 Hz 1 ch 279 ms |
| `ogg->ogg` | audio | PASS | tags=carried strip=clean | vorbis 44100 Hz 1 ch 247 ms |
| `ogg->opus` | audio | PASS | tags=carried strip=clean | opus 48000 Hz 1 ch 254 ms |
| `opus->mp3` | audio | PASS | tags=carried strip=clean | mp3 48000 Hz 1 ch 250 ms |
| `opus->wav` | audio | PASS | tags=carried strip=clean | pcm_s16le 48000 Hz 1 ch 250 ms |
| `opus->flac` | audio | PASS | tags=carried strip=clean | flac 48000 Hz 1 ch 250 ms |
| `opus->m4a` | audio | PASS | tags=carried strip=clean | aac 48000 Hz 1 ch 250 ms |
| `opus->aac` | audio | PASS | tags=carried strip=clean | aac 48000 Hz 1 ch 277 ms |
| `opus->ogg` | audio | PASS | tags=carried strip=clean | vorbis 48000 Hz 1 ch 250 ms |
| `opus->opus` | audio | PASS | tags=carried strip=clean | opus 48000 Hz 1 ch 257 ms |
| `mp4->mp4` | video | PASS | n/a: SPEC §5 names no video metadata field | h264 64x48 1000 ms |
| `mp4->mkv` | video | PASS | n/a: SPEC §5 names no video metadata field | h264 64x48 1000 ms |
| `mp4->mov` | video | PASS | n/a: SPEC §5 names no video metadata field | h264 64x48 1000 ms |
| `mp4->webm` | video | PASS | n/a: SPEC §5 names no video metadata field | vp9 64x48 1000 ms |
| `mp4->gif` | video | PASS | n/a: SPEC §5 names no video metadata field | gif 64x48 1000 ms |
| `mkv->mp4` | video | PASS | n/a: SPEC §5 names no video metadata field | h264 64x48 1000 ms |
| `mkv->mkv` | video | PASS | n/a: SPEC §5 names no video metadata field | h264 64x48 1000 ms |
| `mkv->mov` | video | PASS | n/a: SPEC §5 names no video metadata field | h264 64x48 1000 ms |
| `mkv->webm` | video | PASS | n/a: SPEC §5 names no video metadata field | vp9 64x48 1000 ms |
| `mkv->gif` | video | PASS | n/a: SPEC §5 names no video metadata field | gif 64x48 1000 ms |
| `mov->mp4` | video | PASS | n/a: SPEC §5 names no video metadata field | h264 64x48 1000 ms |
| `mov->mkv` | video | PASS | n/a: SPEC §5 names no video metadata field | h264 64x48 1000 ms |
| `mov->mov` | video | PASS | n/a: SPEC §5 names no video metadata field | h264 64x48 1000 ms |
| `mov->webm` | video | PASS | n/a: SPEC §5 names no video metadata field | vp9 64x48 1000 ms |
| `mov->gif` | video | PASS | n/a: SPEC §5 names no video metadata field | gif 64x48 1000 ms |
| `webm->mp4` | video | PASS | n/a: SPEC §5 names no video metadata field | h264 64x48 1000 ms |
| `webm->mkv` | video | PASS | n/a: SPEC §5 names no video metadata field | h264 64x48 1000 ms |
| `webm->mov` | video | PASS | n/a: SPEC §5 names no video metadata field | h264 64x48 1000 ms |
| `webm->webm` | video | PASS | n/a: SPEC §5 names no video metadata field | vp9 64x48 1000 ms |
| `webm->gif` | video | PASS | n/a: SPEC §5 names no video metadata field | gif 64x48 1000 ms |
| `csv->pdf` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | 1 page(s) |
| `csv->txt` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | 105 B of text |
| `csv->xlsx` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | xlsx container with xl/workbook.xml |
| `doc->docx` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | docx container with word/document.xml |
| `doc->html` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | 4352 B of html |
| `doc->md` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | 385 B of markdown (heading present) |
| `doc->pdf` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | 1 page(s) |
| `doc->rtf` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | 23116 B of text |
| `doc->txt` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | 341 B of text |
| `docx->html` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | 4064 B of html |
| `docx->md` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | 389 B of markdown (heading present) |
| `docx->pdf` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | 1 page(s) |
| `docx->rtf` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | 22392 B of text |
| `docx->txt` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | 341 B of text |
| `html->docx` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | docx container with word/document.xml |
| `html->md` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | 27 B of markdown (heading present) |
| `html->pdf` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | 1 page(s) |
| `html->rtf` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | 3254 B of text |
| `md->docx` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | docx container with word/document.xml |
| `md->html` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | 1336 B of html |
| `md->pdf` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | 1 page(s) |
| `md->rtf` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | 5505 B of text |
| `md->txt` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | 124 B of text |
| `ppt->html` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | 932 B of html |
| `ppt->pdf` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | 2 page(s) |
| `ppt->pptx` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | pptx container with ppt/presentation.xml |
| `pptx->html` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | 917 B of html |
| `pptx->pdf` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | 2 page(s) |
| `rtf->docx` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | docx container with word/document.xml |
| `rtf->html` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | 668 B of html |
| `rtf->md` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | 30 B of markdown (text extracted) |
| `rtf->pdf` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | 1 page(s) |
| `rtf->txt` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | 33 B of text |
| `xls->csv` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | 147 B of csv as xls-to-csv.csv, source cell present |
| `xls->html` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | 5235 B of html |
| `xls->pdf` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | 2 page(s) |
| `xls->txt` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | 145 B of text |
| `xls->xlsx` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | xlsx container with xl/workbook.xml |
| `xlsx->csv` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | 147 B of csv as xlsx-to-csv.csv, source cell present |
| `xlsx->html` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | 5655 B of html |
| `xlsx->pdf` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | 2 page(s) |
| `xlsx->txt` | document | PASS | n/a: SPEC §5 policy fields are image EXIF/ICC and audio tags | 145 B of text |
