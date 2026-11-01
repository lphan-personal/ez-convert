param(
  [string]$Viewer = 'edge',
  [string]$Pdf    = 'C:\build\s4\pdf\out\sample.pdf',
  [string]$OutDir = 'C:\build\s4\gui',
  [string]$Log    = ''
)
# v2.ps1 — drive a REAL viewer on the win11-dev VM (runs in session 2 via launcher.ps1).
# Measures, per viewer: does the page render, does Ctrl+F find 'café', and what does Ctrl+A/Ctrl+C
# put on the clipboard (i.e. the text layer a user actually gets). Helper block below is the
# proven acro10 driver (window enumerate/focus/screenshot/clipboard), unchanged.
# Acrobat Reader, sixth pass. Root cause of every failed pass is now identified:
# a modal AVL_AVDialog ("Always open PDFs in Acrobat Reader?") owns the foreground, so the
# document window can never take focus and Ctrl+A/Ctrl+C go nowhere. Dismiss it by name, then
# measure. Nothing about the PDF or the harness changes.
$ErrorActionPreference = 'Continue'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
using System.Collections.Generic;
public class W9 {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern void SwitchToThisWindow(IntPtr h, bool alt);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int dx, int dy, uint d, IntPtr e);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint a, uint b, bool f);
  [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumWindowsProc cb, IntPtr l);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassNameW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
  delegate bool EnumWindowsProc(IntPtr h, IntPtr l);
  public static List<IntPtr> FindAllByClass(string cls) {
    var res = new List<IntPtr>();
    EnumWindows(delegate(IntPtr h, IntPtr l) {
      if (!IsWindowVisible(h)) return true;
      var cn = new StringBuilder(256); GetClassNameW(h, cn, 256);
      if (cn.ToString() == cls) res.Add(h);
      return true;
    }, IntPtr.Zero);
    return res;
  }
  public static string Describe(IntPtr want) {
    string res = "unknown";
    EnumWindows(delegate(IntPtr h, IntPtr l) {
      if (h != want) return true;
      var cn = new StringBuilder(256); GetClassNameW(h, cn, 256);
      var tt = new StringBuilder(512); GetWindowTextW(h, tt, 512);
      uint pid = 0; GetWindowThreadProcessId(h, out pid);
      string pname = "";
      try { pname = System.Diagnostics.Process.GetProcessById((int)pid).ProcessName; } catch {}
      res = "pid=" + pid + " proc=" + pname + " class=" + cn + " title=" + tt;
      return false;
    }, IntPtr.Zero);
    return res;
  }
}
public class C9 {
  [DllImport("user32.dll", SetLastError=true)] static extern bool OpenClipboard(IntPtr o);
  [DllImport("user32.dll", SetLastError=true)] static extern bool CloseClipboard();
  [DllImport("user32.dll", SetLastError=true)] static extern bool EmptyClipboard();
  [DllImport("user32.dll", SetLastError=true)] static extern IntPtr GetClipboardData(uint f);
  [DllImport("user32.dll", SetLastError=true)] static extern IntPtr SetClipboardData(uint f, IntPtr m);
  [DllImport("kernel32.dll", SetLastError=true)] static extern IntPtr GlobalAlloc(uint f, UIntPtr b);
  [DllImport("kernel32.dll", SetLastError=true)] static extern IntPtr GlobalLock(IntPtr m);
  [DllImport("kernel32.dll", SetLastError=true)] static extern bool GlobalUnlock(IntPtr m);
  const uint CF_UNICODETEXT = 13; const uint GMEM_MOVEABLE = 0x0002;
  public static string GetText(int tries, out string err) {
    err = "";
    for (int i = 0; i < tries; i++) {
      if (OpenClipboard(IntPtr.Zero)) {
        try {
          IntPtr h = GetClipboardData(CF_UNICODETEXT);
          if (h == IntPtr.Zero) { err = "no CF_UNICODETEXT"; return null; }
          IntPtr p = GlobalLock(h); if (p == IntPtr.Zero) { err = "GlobalLock"; return null; }
          string s = Marshal.PtrToStringUni(p); GlobalUnlock(h); return s;
        } finally { CloseClipboard(); }
      }
      err = "OpenClipboard err=" + Marshal.GetLastWin32Error();
      System.Threading.Thread.Sleep(400);
    }
    return null;
  }
  public static bool SetText(string t) {
    for (int i = 0; i < 8; i++) {
      if (!OpenClipboard(IntPtr.Zero)) { System.Threading.Thread.Sleep(400); continue; }
      try {
        EmptyClipboard();
        byte[] b = Encoding.Unicode.GetBytes(t + "\0");
        IntPtr h = GlobalAlloc(GMEM_MOVEABLE, (UIntPtr)b.Length);
        IntPtr p = GlobalLock(h); Marshal.Copy(b, 0, p, b.Length); GlobalUnlock(h);
        if (SetClipboardData(CF_UNICODETEXT, h) == IntPtr.Zero) return false;
        return true;
      } finally { CloseClipboard(); }
    }
    return false;
  }
}
'@
New-Item -ItemType Directory -Path $OutDir -Force | Out-Null
[void][W9]::SetProcessDPIAware()
function Say([string]$m) { $m | Tee-Object -FilePath $Log -Append | Out-Null; Write-Output $m }
function Esc([string]$s) {
  if ($null -eq $s) { return '<null>' }
  $out = ''; $n = [Math]::Min(340, $s.Length)
  for ($i = 0; $i -lt $n; $i++) { $c = $s[$i]; $code = [int]$c
    if ($code -lt 32 -or $code -gt 126) { $out += ('[' + $code.ToString('X4') + ']') } else { $out += $c } }
  return $out
}
function Shot([IntPtr]$hh, [string]$path) {
  $r = [W9+RECT]::new(); [void][W9]::GetWindowRect($hh, [ref]$r)
  $bmp = New-Object System.Drawing.Bitmap ($r.Right - $r.Left), ($r.Bottom - $r.Top)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  try { $g.CopyFromScreen($r.Left, $r.Top, 0, 0, $bmp.Size) } catch { Say ('  shot err ' + $_.Exception.Message) }
  $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png); $g.Dispose(); $bmp.Dispose()
  Say ('  shot ' + $path)
}
function Focus([IntPtr]$hh) {
  for ($i = 1; $i -le 6; $i++) {
    [void][W9]::ShowWindow($hh, 9)
    $fg = [W9]::GetForegroundWindow(); $fgPid = [uint32]0
    $t1 = [W9]::GetWindowThreadProcessId($fg, [ref]$fgPid); $me = [W9]::GetCurrentThreadId()
    [void][W9]::AttachThreadInput($me, $t1, $true)
    [void][W9]::BringWindowToTop($hh); [void][W9]::SetForegroundWindow($hh); [void][W9]::SwitchToThisWindow($hh, $true)
    [void][W9]::AttachThreadInput($me, $t1, $false)
    Start-Sleep -Milliseconds 800
    $now = [W9]::GetForegroundWindow()
    if ($now -eq $hh) { Say ('  focus OK on attempt ' + $i); return $true }
    Say ('  focus attempt ' + $i + ': fg=' + $now + ' (' + [W9]::Describe($now) + ')')
    [System.Windows.Forms.SendKeys]::SendWait('%{TAB}'); Start-Sleep -Milliseconds 900
    if ([W9]::GetForegroundWindow() -eq $hh) { Say ('  focus OK via Alt+Tab on attempt ' + $i); return $true }
  }
  return $false
}
# Dismiss Acrobat's own modal prompts. Acrobat draws them natively (class AVL_AVDialog) and
# exposes no UIA buttons, so it is driven with keys: Escape and Alt+N both do nothing, Tab+Enter
# closes it (verified by acro_probe2.ps1, 2026-11-01; the default PDF handler stayed MSEdgePDF,
# i.e. the prompt's "No" was taken).
function Dismiss-Dialogs {
  $n = 0
  for ($round = 1; $round -le 8; $round++) {
    $dlgs = [W9]::FindAllByClass('AVL_AVDialog')
    if ($dlgs.Count -eq 0) { break }
    foreach ($dh in $dlgs) {
      $dr = [W9+RECT]::new(); [void][W9]::GetWindowRect($dh, [ref]$dr)
      $w = $dr.Right - $dr.Left; $ht = $dr.Bottom - $dr.Top
      Say ('  dialog ' + $dh + ' (' + [W9]::Describe($dh) + ') size=' + $w + 'x' + $ht)
      # record what the dialog looks like, so every dismissal is documented
      $bmp = New-Object System.Drawing.Bitmap $w, $ht
      $g = [System.Drawing.Graphics]::FromImage($bmp)
      try { $g.CopyFromScreen($dr.Left, $dr.Top, 0, 0, $bmp.Size) } catch {}
      $bmp.Save((Join-Path $OutDir ('acrobat10-dialog-' + $round + '.png')), [System.Drawing.Imaging.ImageFormat]::Png)
      $g.Dispose(); $bmp.Dispose()
      [void][W9]::SetForegroundWindow($dh)
      Start-Sleep -Milliseconds 400
      if ($w -ge 560 -and $w -le 640 -and $ht -ge 190 -and $ht -le 250) {
        # the default-PDF prompt: "No" centre measured at 73.8% / 78.2% of the dialog rect
        $bx = [int]($dr.Left + $w * 0.738); $by = [int]($dr.Top + $ht * 0.782)
        for ($c = 1; $c -le 2; $c++) {
          [void][W9]::SetCursorPos($bx, $by); Start-Sleep -Milliseconds 300
          [W9]::mouse_event(0x0002, 0, 0, 0, [IntPtr]::Zero); Start-Sleep -Milliseconds 120
          [W9]::mouse_event(0x0004, 0, 0, 0, [IntPtr]::Zero); Start-Sleep -Milliseconds 600
        }
        Say ('  clicked No at ' + $bx + ',' + $by)
      } else {
        # a "Welcome …" panel and friends: Escape first, then Tab+Enter
        [System.Windows.Forms.SendKeys]::SendWait('{ESC}'); Start-Sleep -Seconds 2
        if ([W9]::FindAllByClass('AVL_AVDialog').Count -gt 0) {
          [System.Windows.Forms.SendKeys]::SendWait('{TAB}'); Start-Sleep -Milliseconds 600
          [System.Windows.Forms.SendKeys]::SendWait('{ENTER}'); Start-Sleep -Seconds 2
        }
      }
    }
    Start-Sleep -Seconds 2
    $n++
    Say ('  dismiss round ' + $round + ': dialogs now=' + ([W9]::FindAllByClass('AVL_AVDialog').Count))
  }
  return $n
}
if (-not $Log) { $Log = Join-Path $OutDir ('v2-' + $Viewer + '.log') }
$cafe  = 'caf' + [char]0x00E9
$naive = 'na' + [char]0x00EF + 've'
$prof  = 'C:\build\s4\profile\v2-' + $Viewer
if ($Viewer -eq 'firefox') {
  # a first-run "Welcome to Firefox" tab steals the tab and Ctrl+A copies its text — pre-seed prefs
  $prof = 'C:\build\s4\profile\v4-firefox'
  New-Item -ItemType Directory -Force -Path $prof | Out-Null
  Set-Content -Encoding ASCII -Path (Join-Path $prof 'user.js') -Value @(
    'user_pref("browser.aboutwelcome.enabled", false);',
    'user_pref("browser.shell.checkDefaultBrowser", false);',
    'user_pref("datareporting.policy.dataSubmissionEnabled", false);',
    'user_pref("toolkit.telemetry.reportingpolicy.firstRun", false);',
    'user_pref("browser.startup.homepage_override.mstone", "ignore");'
  )
}
switch ($Viewer) {
  'edge'    { $exe = 'C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe';       $cls = 'Chrome_WidgetWin_1'; $xargs = @("--user-data-dir=$prof", '--no-first-run', '--no-default-browser-check', $Pdf); $proc = 'msedge' }
  'chrome'  { $exe = 'C:\Program Files\Google\Chrome\Application\chrome.exe';              $cls = 'Chrome_WidgetWin_1'; $xargs = @("--user-data-dir=$prof", '--no-first-run', '--no-default-browser-check', $Pdf); $proc = 'chrome' }
  'firefox' { $exe = 'C:\Program Files\Mozilla Firefox\firefox.exe';                       $cls = 'MozillaWindowClass'; $xargs = @('-no-remote', '-profile', $prof, '-new-window', $Pdf); $proc = 'firefox' }
  'acrobat' { $exe = 'C:\Program Files\Adobe\Acrobat DC\Acrobat\Acrobat.exe';              $cls = 'AcrobatSDIWindow';   $xargs = @($Pdf); $proc = 'Acrobat' }
  default   { throw 'unknown viewer' }
}
Say ('=== v2 ' + $Viewer + ' ' + (Get-Date -Format o) + '  session=' + (Get-Process -Id $PID).SessionId)
Say ('  exe=' + $exe + '  exists=' + (Test-Path $exe))
Say ('  pdf=' + $Pdf + '  size=' + (Get-Item $Pdf).Length + '  sha=' + (Get-FileHash -Algorithm SHA256 $Pdf).Hash)
Get-Process $proc -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 3

try { Start-Process -FilePath $exe -ArgumentList $xargs | Out-Null } catch { Say ('  LAUNCH FAILED: ' + $_.Exception.Message); exit 21 }

function Find-Win([string]$c, [int]$tmo) {
  for ($i = 0; $i -lt $tmo; $i++) {
    Start-Sleep -Seconds 1
    foreach ($h in [W9]::FindAllByClass($c)) {
      $t = [W9]::Describe($h)
      if ($t -match 'sample' -or $t -match 'pdf') { return $h }
    }
  }
  $all = [W9]::FindAllByClass($c)
  if ($all.Count -gt 0) { return $all[0] }
  return [IntPtr]::Zero
}
$h = Find-Win $cls 60
if ($h -eq [IntPtr]::Zero) {
  Say '  NO WINDOW - windows seen:'
  foreach ($p in (Get-Process | Where-Object { $_.MainWindowHandle -ne 0 })) { Say ('    ' + $p.ProcessName + ' hwnd=' + $p.MainWindowHandle + ' title=' + $p.MainWindowTitle) }
  exit 20
}
Say ('  window: ' + [W9]::Describe($h))
Start-Sleep -Seconds 12
if ($Viewer -eq 'acrobat') { [void](Dismiss-Dialogs); Start-Sleep -Seconds 3 }
$f = Focus $h
Say ('  focus=' + $f + '  fg=' + [W9]::Describe([W9]::GetForegroundWindow()))
Shot $h (Join-Path $OutDir ('v2-' + $Viewer + '-1-render.png'))
$r = [W9+RECT]::new(); [void][W9]::GetWindowRect($h, [ref]$r)
Say ('  rect=' + $r.Left + ',' + $r.Top + '..' + $r.Right + ',' + $r.Bottom)
if ($Viewer -eq 'firefox') {
  # Firefox 159 opens a first-run "Welcome to Firefox" MODAL over the PDF (not a tab); it swallows
  # every keystroke, so the find bar stays empty and Ctrl+A copies the modal's own text. Enter is
  # inert; the purple "Continue" button sits at 50.0% x / 47.8% y of the window (measured from
  # v2-firefox-1b-after-enter.png).
  $Wf = $r.Right - $r.Left; $Hf = $r.Bottom - $r.Top
  for ($c = 1; $c -le 2; $c++) {
    [void][W9]::SetCursorPos([int]($r.Left + $Wf * 0.500), [int]($r.Top + $Hf * 0.478))
    Start-Sleep -Milliseconds 500
    [W9]::mouse_event(0x0002, 0, 0, 0, [IntPtr]::Zero); Start-Sleep -Milliseconds 150
    [W9]::mouse_event(0x0004, 0, 0, 0, [IntPtr]::Zero); Start-Sleep -Seconds 2
  }
  [void](Focus $h)
  Start-Sleep -Seconds 2
  Shot $h (Join-Path $OutDir ('v2-' + $Viewer + '-1b-after-continue.png'))
}

function UiA([string]$tag) {
  try {
    $root = [System.Windows.Automation.AutomationElement]::FromHandle($h)
    $all = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
    $lines = New-Object System.Collections.Generic.List[string]
    foreach ($e in $all) {
      try { $nm = $e.Current.Name; if ($nm) { $lines.Add(($e.Current.ControlType.ProgrammaticName + ' | name=' + $nm)) } } catch {}
    }
    [IO.File]::WriteAllLines((Join-Path $OutDir ('v2-' + $Viewer + '-' + $tag + '-uia.txt')), $lines, (New-Object System.Text.UTF8Encoding $false))
    foreach ($l in $lines) { if ($l -match '(?i)(\d+\s*(of|/)\s*\d+|no exact matches|no matches|no results|match)') { Say ('  UIA[' + $tag + '] ' + (Esc $l)) } }
  } catch { Say ('  UIA failed: ' + $_.Exception.Message) }
}

# --- find 'café' (and an ASCII control query, to prove the harness drives the find UI at all)
function Find-Text([string]$q, [string]$tag) {
  [void](Focus $h)
  [void][C9]::SetText($q); Start-Sleep -Milliseconds 400
  [System.Windows.Forms.SendKeys]::SendWait('^f'); Start-Sleep -Seconds 2
  [System.Windows.Forms.SendKeys]::SendWait('^v'); Start-Sleep -Seconds 2
  [System.Windows.Forms.SendKeys]::SendWait('{ENTER}'); Start-Sleep -Seconds 4
  Shot $h (Join-Path $OutDir ('v2-' + $Viewer + '-2-find-' + $tag + '.png'))
  UiA ('2-find-' + $tag)
  [System.Windows.Forms.SendKeys]::SendWait('{ESC}'); Start-Sleep -Seconds 1
}
Find-Text 'MARKER-ALPHA' 'control-ascii'
Find-Text $cafe 'cafe'

# --- copy: click into the page, select all, copy, read the clipboard
$results = @()
for ($attempt = 1; $attempt -le 2; $attempt++) {
  if ($Viewer -eq 'acrobat') { [void](Dismiss-Dialogs) }
  $fa = Focus $h
  for ($c = 1; $c -le 2; $c++) {
    [void][W9]::SetCursorPos([int](($r.Left + $r.Right) / 2), [int]($r.Top + ($r.Bottom - $r.Top) * 0.40))
    Start-Sleep -Milliseconds 400
    [W9]::mouse_event(0x0002, 0, 0, 0, [IntPtr]::Zero); Start-Sleep -Milliseconds 150
    [W9]::mouse_event(0x0004, 0, 0, 0, [IntPtr]::Zero); Start-Sleep -Milliseconds 700
  }
  [void][C9]::SetText('SENTINEL-NOTHING-COPIED'); Start-Sleep -Milliseconds 400
  [System.Windows.Forms.SendKeys]::SendWait('^a'); Start-Sleep -Seconds 3
  Shot $h (Join-Path $OutDir ('v2-' + $Viewer + '-3-selected-' + $attempt + '.png'))
  [System.Windows.Forms.SendKeys]::SendWait('^c'); Start-Sleep -Seconds 4
  $clip = [C9]::GetText(10, [ref]$null); if ($null -eq $clip) { $clip = '' }
  [IO.File]::WriteAllText((Join-Path $OutDir ('v2-' + $Viewer + '-3-clipboard-' + $attempt + '.txt')), $clip, (New-Object System.Text.UTF8Encoding $false))
  $line = 'copy ' + $attempt + ' focus=' + $fa + ' len=' + $clip.Length + ' sentinel=' + $clip.Contains('SENTINEL') + ' cafe=' + $clip.Contains($cafe) + ' naive=' + $clip.Contains($naive) + ' marker=' + $clip.Contains('MARKER-ALPHA') + ' ctrl1C=' + $clip.Contains([char]0x1C) + ' ctrl1F=' + $clip.Contains([char]0x1F)
  $results += $line
  Say ('  ' + $line)
  Say ('  TEXT: ' + (Esc $clip))
}
$results | Set-Content -Encoding UTF8 (Join-Path $OutDir ('v2-' + $Viewer + '-summary.txt'))
Get-Process $proc -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Say '=== v2 done'
