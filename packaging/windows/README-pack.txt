EZ Convert — Windows Engine Pack
===============================

EZ Convert converts files entirely on this PC: no network calls, no telemetry.
The conversion engines (libvips, ffmpeg, LibreOffice) are too large to live in
the app itself, so they ship in this pack and are installed once.

Install
-------
1. Unzip this archive into the EZ Convert install folder, merging with what is
   already there:

     %LOCALAPPDATA%\EZ Convert\

   The folder must end up with `engines\` and `licenses\` beside `ez-convert.exe`.
   (Explorer: right-click the zip -> Extract All -> point it at that folder.)
2. Start EZ Convert. The engine row in Settings should show all three engines.

To check the installation from a command prompt:

  cd /d "%LOCALAPPDATA%\EZ Convert"
  ez-convert.exe --diagnostics --fixtures .

To uninstall, delete the folder `%LOCALAPPDATA%\EZ Convert\engines\soffice`.

What is inside
--------------
  engines\vips     libvips 8.18.7 + its DLLs (LGPL-2.1-or-later), built without
                   the GPL imagequant/poppler modules; HEIC/AVIF decode included
  engines\ffmpeg   ffmpeg/ffprobe, LGPL build without GPL components (LGPL-3.0)
  engines\soffice  LibreOffice, separate process (MPL-2.0)

Every file is listed in MANIFEST.sha256. Licence texts and the exact build record
(the ffmpeg configure output, the LibreOffice MSI digest, the list of files the
licence decision stripped, and the licence-gate scan) are in `licenses\`.

The app never links these engines: each one is a separate process, launched from
its own folder. The binaries are not code-signed in this build — Windows may show
a SmartScreen warning the first time; choose "More info" -> "Run anyway", or see
docs/DISTRIBUTION.md for the signed-release path.
