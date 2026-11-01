param([string]$Script = 'v2.ps1', [string]$Viewer = 'edge', [int]$WaitSec = 300, [string]$Tag = '')
# runany.ps1 — (session 0) SYSTEM task → launcher.ps1 → $Script as the interactive user (session 2).
$ErrorActionPreference = 'Continue'
$tag = if ($Tag) { $Tag } else { $Script -replace '\.ps1$', '' }
$child = 'C:\build\s4\_child_' + $tag + '.ps1'
$call = "& 'C:\build\s4\" + $Script + "'"
if ($Viewer) { $call += " -Viewer '" + $Viewer + "'" }
Set-Content -Encoding ASCII -Path $child -Value $call
$launchLog = 'C:\build\s4\gui\launcher-' + $tag + '.log'
if (Test-Path $launchLog) { Remove-Item $launchLog -Force }
$tn = 's4v2'
$tr = 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\build\s4\launcher.ps1 -Child ' + $child + ' -Log ' + $launchLog
& schtasks /create /tn $tn /tr $tr /sc once /st 23:59 /ru SYSTEM /rl HIGHEST /f | Out-Null
"task created rc=$LASTEXITCODE  child=$child"
& schtasks /run /tn $tn | Out-Null
"task started  rc=$LASTEXITCODE"
$waited = 0
for ($i = 0; $i -lt [int]($WaitSec / 5); $i++) {
  Start-Sleep -Seconds 5
  $waited += 5
  if (Test-Path $launchLog) {
    $txt = Get-Content $launchLog -Raw -ErrorAction SilentlyContinue
    if ($txt -match 'child exited') { break }
  }
}
"waited       : $waited s"
"---- launcher ----"
if (Test-Path $launchLog) { Get-Content $launchLog | ForEach-Object { "  " + $_ } }
