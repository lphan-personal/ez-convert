# convert.ps1 — S4 item 2: docx -> pdf on Windows with the S3-proven argv.
$ErrorActionPreference = 'Stop'
$root = 'C:\build\s4\pdf'
$in   = Join-Path $root 'in\sample.docx'
$out  = Join-Path $root 'out'
$prof = 'file:///C:/build/s4/pdf/profile'
$soffice = 'C:\Program Files\LibreOffice\program\soffice.com'

Write-Output ("soffice       : " + $soffice + "  exists=" + (Test-Path $soffice))
Write-Output ("input         : " + $in + "  size=" + (Get-Item $in).Length + "  sha256=" + (Get-FileHash -Algorithm SHA256 $in).Hash)
if (Test-Path $out) { Remove-Item -Recurse -Force $out }
New-Item -ItemType Directory -Path $out -Force | Out-Null

$sw = [Diagnostics.Stopwatch]::StartNew()
$stdout = & $soffice --headless --norestore --nolockcheck --nodefault --nologo `
    "-env:UserInstallation=$prof" --convert-to 'pdf:writer_pdf_Export' --outdir $out $in 2>&1
$rc = $LASTEXITCODE
$sw.Stop()

Write-Output ("rc            : " + $rc)
Write-Output ("wall_ms       : " + $sw.ElapsedMilliseconds)
Write-Output ("stdout/stderr :")
$stdout | ForEach-Object { Write-Output ("  " + $_) }
Get-ChildItem $out | ForEach-Object {
    Write-Output ("output        : " + $_.Name + "  " + $_.Length + " B  sha256=" + (Get-FileHash -Algorithm SHA256 $_.FullName).Hash)
}
