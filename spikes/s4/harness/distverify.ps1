$ErrorActionPreference='Continue'
# verify the dist on THIS host against the S2c/S4 manifest before the suite runs
$man = 'C:\build\s4\FINAL-dist-spec-aom-sha256.txt'
$root = 'C:\build\dist'
$entries = @{}
foreach ($line in Get-Content $man) {
  if ($line.StartsWith('#')) { continue }
  if ($line -match '^([0-9a-f]{64})\s+(\d+)\s+\./(.+)$') { $entries[$Matches[3]] = @{ hash = $Matches[1]; size = [int64]$Matches[2] } }
}
"manifest entries : " + $entries.Count
$files = Get-ChildItem $root -Recurse -File
"files on disk    : " + $files.Count
$bad = 0; $missing = 0; $extra = @()
foreach ($f in $files) {
  $rel = $f.FullName.Substring($root.Length + 1).Replace('\', '/')
  if (-not $entries.ContainsKey($rel)) { $extra += $rel; continue }
  $h = (Get-FileHash -Algorithm SHA256 $f.FullName).Hash.ToLower()
  if ($h -ne $entries[$rel].hash -or $f.Length -ne $entries[$rel].size) { $bad++; "MISMATCH  $rel" }
}
foreach ($k in $entries.Keys) { if (-not (Test-Path (Join-Path $root ($k -replace '/', '\')))) { $missing++; "MISSING   $k" } }
"hash mismatches  : " + $bad
"missing          : " + $missing
"not in manifest  : " + ($extra -join ', ')
"total bytes      : " + (($files | Measure-Object Length -Sum).Sum)
"libvips-42.dll   : " + (Get-FileHash -Algorithm SHA256 "$root\libvips-42.dll").Hash.ToLower()
"libheif.dll      : " + (Get-FileHash -Algorithm SHA256 "$root\libheif.dll").Hash.ToLower()
if ($bad -eq 0 -and $missing -eq 0 -and $extra.Count -eq 0) { "VERDICT          : dist is byte-identical to the manifest" } else { "VERDICT          : DIST DOES NOT MATCH" }
