param(
  [string]$Pdf    = 'C:\build\s4\pdf\out\sample.pdf',
  [string]$OutDir = 'C:\build\s4\gui',
  [string]$Log    = 'C:\build\s4\gui\viewer_test2.log'
)
# v2: Win32 clipboard (CF_UNICODETEXT) so nothing depends on cmdlet encoding; ASCII-only source;
#     --force-renderer-accessibility so UIA exposes the PDF text layer and the find result.
$ErrorActionPreference = 'Continue'
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public class Win {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int dx, int dy, uint d, IntPtr e);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint a, uint b, bool f);
  [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
  [DllImport("user32.dll")] public static extern IntPtr GetWindowTextW(IntPtr h, StringBuilder s, int n);
}
public class Clip {
  [DllImport("user32.dll", SetLastError=true)] static extern bool OpenClipboard(IntPtr hWndNewOwner);
  [DllImport("user32.dll", SetLastError=true)] static extern bool CloseClipboard();
  [DllImport("user32.dll", SetLastError=true)] static extern bool EmptyClipboard();
  [DllImport("user32.dll", SetLastError=true)] static extern IntPtr GetClipboardData(uint uFormat);
  [DllImport("user32.dll", SetLastError=true)] static extern IntPtr SetClipboardData(uint uFormat, IntPtr hMem);
  [DllImport("kernel32.dll", SetLastError=true)] static extern IntPtr GlobalAlloc(uint flags, UIntPtr bytes);
  [DllImport("kernel32.dll", SetLastError=true)] static extern IntPtr GlobalLock(IntPtr hMem);
  [DllImport("kernel32.dll", SetLastError=true)] static extern bool GlobalUnlock(IntPtr hMem);
  [DllImport("kernel32.dll", SetLastError=true)] static extern IntPtr GlobalSize(IntPtr hMem);
  const uint CF_UNICODETEXT = 13;
  const uint GMEM_MOVEABLE = 0x0002;

  public static string GetText(int tries, out string err) {
    err = "";
    for (int i = 0; i < tries; i++) {
      if (OpenClipboard(IntPtr.Zero)) {
        try {
          IntPtr h = GetClipboardData(CF_UNICODETEXT);
          if (h == IntPtr.Zero) { err = "no CF_UNICODETEXT (GetLastError=" + Marshal.GetLastWin32Error() + ")"; return null; }
          IntPtr p = GlobalLock(h);
          if (p == IntPtr.Zero) { err = "GlobalLock failed"; return null; }
          string s = Marshal.PtrToStringUni(p);
          GlobalUnlock(h);
          return s;
        } finally { CloseClipboard(); }
      }
      err = "OpenClipboard failed err=" + Marshal.GetLastWin32Error();
      System.Threading.Thread.Sleep(400);
    }
    return null;
  }
  public static bool SetText(string text) {
    for (int i = 0; i < 8; i++) {
      if (!OpenClipboard(IntPtr.Zero)) { System.Threading.Thread.Sleep(400); continue; }
      try {
        EmptyClipboard();
        byte[] b = Encoding.Unicode.GetBytes(text + "\0");
        IntPtr h = GlobalAlloc(GMEM_MOVEABLE, (UIntPtr)b.Length);
        IntPtr p = GlobalLock(h);
        Marshal.Copy(b, 0, p, b.Length);
        GlobalUnlock(h);
        if (SetClipboardData(CF_UNICODETEXT, h) == IntPtr.Zero) return false;
        return true;
      } finally { CloseClipboard(); }
    }
    return false;
  }
}
'@

New-Item -ItemType Directory -Path $OutDir -Force | Out-Null
function Say([string]$m) { $m | Tee-Object -FilePath $Log -Append | Out-Null; Write-Output $m }
$acc = [char]0x00E9   # e-acute
$cafe = 'caf' + $acc
$naive = 'na' + [char]0x00EF + 've'
Say ("=== viewer_test v2 " + (Get-Date -Format o) + " session=" + [Diagnostics.Process]::GetCurrentProcess().SessionId + " user=" + [Environment]::UserName)
Say ("pdf=" + $Pdf + " exists=" + (Test-Path $Pdf))
# self-test the clipboard path before trusting it
$e = ''
$null = [Clip]::SetText('SELFTEST-' + $cafe)
$rt = [Clip]::GetText(3, [ref]$e)
Say ("clipboard self-test: set='SELFTEST-" + $cafe + "' got='" + $rt + "' err='" + $e + "' ok=" + ($rt -eq ('SELFTEST-' + $cafe)))

function Force-Focus([IntPtr]$h) {
  [void][Win]::ShowWindow($h, 9)
  $fg = [Win]::GetForegroundWindow()
  $fgPid = [uint32]0
  $fgTid = [Win]::GetWindowThreadProcessId($fg, [ref]$fgPid)
  $myTid = [Win]::GetCurrentThreadId()
  [void][Win]::AttachThreadInput($myTid, $fgTid, $true)
  [void][Win]::BringWindowToTop($h)
  [void][Win]::SetForegroundWindow($h)
  [void][Win]::AttachThreadInput($myTid, $fgTid, $false)
  Start-Sleep -Milliseconds 350
}
function Shot([IntPtr]$h, [string]$path) {
  $r = [Win+RECT]::new()
  if (-not [Win]::GetWindowRect($h, [ref]$r)) { Say ("  shot FAILED err=" + [Runtime.InteropServices.Marshal]::GetLastWin32Error()); return }
  $w = $r.Right - $r.Left; $ht = $r.Bottom - $r.Top
  $bmp = New-Object System.Drawing.Bitmap $w, $ht
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  try { $g.CopyFromScreen($r.Left, $r.Top, 0, 0, $bmp.Size) } catch { Say ("  shot copy error: " + $_.Exception.Message) }
  $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
  Say ("  shot " + $path + " " + (Get-Item $path).Length + " B")
}
function Uia-Dump([IntPtr]$h, [string]$path) {
  try {
    $el = [System.Windows.Automation.AutomationElement]::FromHandle($h)
    $all = $el.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
    $lines = New-Object System.Collections.Generic.List[string]
    foreach ($e2 in $all) {
      try {
        $ct = $e2.Current.ControlType.ProgrammaticName
        $nm = $e2.Current.Name
        $val = ''
        try { $vp = $e2.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern); $val = $vp.Current.Value } catch {}
        if ($nm -or $val) { $lines.Add(("{0} | name='{1}' | value='{2}'" -f $ct, $nm, $val)) }
      } catch {}
    }
    [IO.File]::WriteAllLines($path, $lines, (New-Object System.Text.UTF8Encoding $false))
    Say ("  uia " + $path + " " + $lines.Count + " nodes")
  } catch { Say ("  uia dump failed: " + $_.Exception.Message) }
}
function Click-Center([IntPtr]$h) {
  $r = [Win+RECT]::new(); [void][Win]::GetWindowRect($h, [ref]$r)
  [void][Win]::SetCursorPos([int](($r.Left + $r.Right)/2), [int](($r.Top + $r.Bottom)/2))
  Start-Sleep -Milliseconds 250
  [Win]::mouse_event(0x0002, 0, 0, 0, [IntPtr]::Zero)
  Start-Sleep -Milliseconds 90
  [Win]::mouse_event(0x0004, 0, 0, 0, [IntPtr]::Zero)
  Start-Sleep -Milliseconds 700
}
function Find-Needle([IntPtr]$h, [string]$needle, [string]$tag) {
  [void][Clip]::SetText($needle)
  Start-Sleep -Milliseconds 400
  [System.Windows.Forms.SendKeys]::SendWait('^f')
  Start-Sleep -Milliseconds 900
  [System.Windows.Forms.SendKeys]::SendWait('^a')
  Start-Sleep -Milliseconds 200
  [System.Windows.Forms.SendKeys]::SendWait('^v')
  Start-Sleep -Milliseconds 2500
  Shot $h (Join-Path $OutDir ($tag + "-find.png"))
  Uia-Dump $h (Join-Path $OutDir ($tag + "-uia-find.txt"))
  [System.Windows.Forms.SendKeys]::SendWait('{ESC}')
  Start-Sleep -Milliseconds 600
}

$viewers = @(
  @{ Name='Chrome'; Exe='C:\Program Files\Google\Chrome\Application\chrome.exe' },
  @{ Name='Edge';   Exe='C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe' },
  @{ Name='Brave';  Exe='C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe' }
)

$results = @()
foreach ($v in $viewers) {
  $name = $v.Name
  Say ("######## " + $name + " ########")
  if (-not (Test-Path $v.Exe)) { Say ("  MISSING exe"); continue }
  $ver = (Get-Item $v.Exe).VersionInfo.ProductVersion
  $prof = Join-Path $OutDir ("profile-" + $name.ToLower())
  if (Test-Path $prof) { Remove-Item -Recurse -Force $prof -ErrorAction SilentlyContinue }
  $uri = 'file:///' + ($Pdf -replace '\\','/')
  $argList = @("--user-data-dir=$prof", '--no-first-run', '--no-default-browser-check',
               '--force-renderer-accessibility', '--disable-features=Translate,MediaRouter',
               '--window-position=40,40', '--window-size=1300,950', '--new-window', $uri)
  $p = Start-Process -FilePath $v.Exe -ArgumentList $argList -PassThru
  $h = [IntPtr]::Zero
  for ($i = 0; $i -lt 60; $i++) {
    Start-Sleep -Milliseconds 500
    $p.Refresh(); if ($p.HasExited) { break }
    if ($p.MainWindowHandle -ne [IntPtr]::Zero) { $h = $p.MainWindowHandle; break }
  }
  if ($h -eq [IntPtr]::Zero) { Say "  NO WINDOW"; continue }
  Say ("  pid=" + $p.Id + " version=" + $ver + " title='" + $p.MainWindowTitle + "'")
  Start-Sleep -Seconds 8
  Force-Focus $h
  Click-Center $h          # give the renderer/PDF plugin focus before any keystroke
  Shot $h (Join-Path $OutDir ($name + "-1-render.png"))

  Find-Needle $h 'MARKER-ALPHA' ($name + "-2-ascii")
  Force-Focus $h
  Find-Needle $h $cafe ($name + "-3-cafe")
  Force-Focus $h

  # select-all + copy from the page
  Click-Center $h
  [void][Clip]::SetText('SENTINEL-NOTHING-COPIED')
  Start-Sleep -Milliseconds 300
  [System.Windows.Forms.SendKeys]::SendWait('^a')
  Start-Sleep -Milliseconds 1000
  [System.Windows.Forms.SendKeys]::SendWait('^c')
  Start-Sleep -Seconds 2
  $err = ''
  $clip = [Clip]::GetText(8, [ref]$err)
  if ($null -eq $clip) { $clip = '' }
  [IO.File]::WriteAllText((Join-Path $OutDir ($name + "-clipboard.txt")), $clip, (New-Object System.Text.UTF8Encoding $false))
  Shot $h (Join-Path $OutDir ($name + "-4-selected.png"))

  $rep = $clip
  if ($rep.Length -gt 400) { $rep = $rep.Substring(0, 400) }
  $esc = -join ($rep.ToCharArray() | ForEach-Object {
      if ([int]$_ -lt 32 -or [int]$_ -gt 126) { '[' + ([int]$_).ToString('X4') + ']' } else { $_ } })
  $row = [ordered]@{
    viewer          = $name
    version         = $ver
    clipboard_len   = $clip.Length
    clipboard_err   = $err
    clipboard_ascii = $esc
    has_cafe        = $clip.Contains($cafe)
    has_0x1c        = $clip.Contains([char]0x1C)
    has_0x1f        = $clip.Contains([char]0x1F)
    has_naive_ok    = $clip.Contains($naive)
    has_cjk         = $clip.Contains([char]0x4E2D)
    has_marker      = $clip.Contains('MARKER-ALPHA')
  }
  $results += [pscustomobject]$row
  Say ("  clip_len=" + $row.clipboard_len + " err='" + $row.clipboard_err + "' cafe=" + $row.has_cafe + " 0x1c=" + $row.has_0x1c + " 0x1f=" + $row.has_0x1f + " cjk=" + $row.has_cjk + " marker=" + $row.has_marker)
  Say ("  clip: " + $esc)

  try { taskkill /PID $p.Id /T /F | Out-Null } catch {}
  Start-Sleep -Seconds 2
}
$results | ConvertTo-Json -Depth 4 | Set-Content -Encoding UTF8 (Join-Path $OutDir 'results2.json')
Say "=== viewer_test v2 done"
