<#
.SYNOPSIS
    s3 on Windows — LibreOffice document direction matrix.

.DESCRIPTION
    Windows arm of spikes/s3/run.sh. Proves the same seven document directions
    as the Linux leg, on the Windows LibreOffice build, and additionally answers
    the platform-dependent questions the Linux leg could not:

      * which executable to spawn — soffice.com (console) vs soffice.exe (GUI)
        vs soffice.bin (the real binary, which crashes with rc=81 on a
        brand-new profile);
      * what the first-ever conversion on a fresh -env:UserInstallation does;
      * what fonts a docx -> pdf actually embeds when real Calibri/Cambria are
        installed (Linux substitutes Carlito/Caladea);
      * real cold/warm wall times on Windows.

    Exits non-zero when a required direction produces no valid file, so a silent
    no-output conversion can never pass.

    This script does NOT install LibreOffice — a normal per-user or full install
    is assumed (bundling is P2's card). It does pip-install the four
    verification-only Python libraries, exactly as run.sh does.

.PARAMETER Out
    Output directory. Default spikes/s3/out-win (never shared with the Linux
    leg's spikes/s3/out, so the two evidence bundles cannot be confused).

.PARAMETER Reps
    Warm repetitions per direction. Default 3, matching the Linux leg.

.PARAMETER NoInstallDeps
    Skip the pip install step (deps already present).

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File spikes/s3/run.ps1
#>
[CmdletBinding()]
param(
    [string]$Out = "",
    [int]$Reps = 3,
    [string]$Soffice = "",
    [switch]$NoInstallDeps
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

# repo root = two levels up from this script (spikes/s3/run.ps1)
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
Set-Location $RepoRoot

if (-not $Out) { $Out = Join-Path $RepoRoot 'spikes\s3\out-win' }
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$Out = (Resolve-Path $Out).Path

Write-Output "== s3 (windows): host =="
$os = Get-CimInstance Win32_OperatingSystem
$cs = Get-CimInstance Win32_ComputerSystem
$osLine = "$($os.Caption) $($os.Version) build $($os.BuildNumber)"
$cpuLine = "$($cs.NumberOfLogicalProcessors) logical cores, $([math]::Round($cs.TotalPhysicalMemory/1GB,1)) GB RAM"
Write-Output "   $osLine"
Write-Output "   $cpuLine"

# ---------------------------------------------------------------- python
Write-Output "== s3 (windows): python =="
$py = $null
$candidates = @(
    (Join-Path $env:LOCALAPPDATA 'Python\bin\python.exe'),
    (Join-Path $env:LOCALAPPDATA 'Programs\Python\Python313\python.exe')
)
foreach ($c in $candidates) { if (Test-Path $c) { $py = $c; break } }
if (-not $py) {
    $cmd = Get-Command py -ErrorAction SilentlyContinue
    if ($cmd) { $py = $cmd.Source }
}
if (-not $py) {
    $cmd = Get-Command python -ErrorAction SilentlyContinue
    if ($cmd) { $py = $cmd.Source }
}
if (-not $py) {
    throw "no python interpreter found; install Python 3.10+ (the spike driver is pure-Python)"
}
Write-Output "   python: $py"
& $py --version
if ($LASTEXITCODE -ne 0) { throw "python does not run: $py" }

# UTF-8 for the child too, so marker text survives a cp1252 console
$env:PYTHONUTF8 = '1'
$env:PYTHONIOENCODING = 'utf-8'

# ------------------------------------------------------- verification deps
if (-not $NoInstallDeps) {
    Write-Output "== s3 (windows): python verification deps =="
    & $py -m pip install --quiet --disable-pip-version-check `
        "python-docx==1.2.0" "openpyxl==3.1.5" "python-pptx==1.0.2" "pypdf==6.21.4"
    if ($LASTEXITCODE -ne 0) { throw "pip install of harness-only deps failed (rc=$LASTEXITCODE)" }
}
& $py -c "import docx, openpyxl, pptx, pypdf; print('   deps ok')"
if ($LASTEXITCODE -ne 0) { throw "harness deps not importable" }

# ---------------------------------------------------------- locate soffice
Write-Output "== s3 (windows): libreoffice =="
function Test-SofficeProgram([string]$dir) {
    if (-not $dir) { return $null }
    # prefer the console-subsystem wrapper: it blocks and yields stdout/stderr
    foreach ($name in @('soffice.com', 'soffice.exe')) {
        $p = Join-Path $dir $name
        if (Test-Path $p) { return $p }
    }
    return $null
}

$found = $null
if ($Soffice -and (Test-Path $Soffice)) {
    $found = $Soffice
} else {
    # (a) registry — works for both machine-wide and per-user MSI installs
    foreach ($key in @(
            'HKLM:\SOFTWARE\LibreOffice\UNO\InstallPath',
            'HKLM:\SOFTWARE\WOW6432Node\LibreOffice\UNO\InstallPath',
            'HKCU:\SOFTWARE\LibreOffice\UNO\InstallPath')) {
        if (Test-Path $key) {
            $prog = (Get-ItemProperty -Path $key).'(default)'
            if ($prog) { $found = Test-SofficeProgram $prog; if ($found) { break } }
        }
    }
    # (b) conventional install roots
    if (-not $found) {
        $roots = @(
            (Join-Path $env:ProgramFiles 'LibreOffice\program'),
            "${env:ProgramFiles(x86)}\LibreOffice\program",
            (Join-Path $env:LOCALAPPDATA 'Programs\LibreOffice\program'),
            (Join-Path $env:LOCALAPPDATA 'LibreOffice\program')
        )
        foreach ($r in $roots) { $found = Test-SofficeProgram $r; if ($found) { break } }
    }
}
if (-not $found) {
    throw "LibreOffice not found. Install LibreOffice 26.8.x for Windows x86-64 first (this script does not bundle it; P2 owns bundling)."
}
Write-Output "   soffice: $found"
$loVersion = (& $found --headless --version 2>&1 | Out-String).Trim()
Write-Output "   version: $loVersion"
$loVersion | Set-Content -Path (Join-Path $Out 'soffice-version.txt') -Encoding UTF8

# what sits next to it — the wrapper-vs-binary facts this spike is about
$progDir = Split-Path $found -Parent
Write-Output "   launchers beside it:"
foreach ($name in @('soffice.com', 'soffice.exe', 'soffice.bin', 'oosplash.exe')) {
    $p = Join-Path $progDir $name
    if (Test-Path $p) {
        $vi = (Get-Item $p).VersionInfo
        Write-Output ("     {0,-13} {1,10} B  fileversion={2}" -f $name, (Get-Item $p).Length, $vi.FileVersion)
    } else {
        Write-Output ("     {0,-13} MISSING" -f $name)
    }
}

# ------------------------------------------------------------------ matrix
Write-Output "== s3 (windows): direction matrix =="
$log = Join-Path $Out 'run.log'
$driverArgs = @(
    (Join-Path 'spikes\s3' 'run_s3.py'),
    '--soffice', $found,
    '--out', $Out,
    '--reps', "$Reps",
    '--controls'
)
$ErrorActionPreference = 'Continue'
& $py @driverArgs 2>&1 | Tee-Object -FilePath $log
$rc = $LASTEXITCODE
$ErrorActionPreference = 'Stop'

Write-Output "== s3 (windows): verdict rc=$rc =="
$table = Join-Path $Out 'table.md'
if (Test-Path $table) { Get-Content $table | ForEach-Object { Write-Output $_ } }

# machine-readable host record alongside the driver's own result.json
$hostInfo = [ordered]@{
    os         = $osLine
    cpu        = $cpuLine
    python     = (& $py --version 2>&1 | Out-String).Trim()
    python_exe = $py
    soffice    = $found
    version    = $loVersion
    reps       = $Reps
    out        = $Out
    exit_code  = $rc
}
$hostInfo | ConvertTo-Json -Depth 4 | Set-Content -Path (Join-Path $Out 'windows-host.json') -Encoding UTF8

exit $rc
