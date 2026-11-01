param(
  [string]$Pdf    = 'C:\build\s4\pdf\out\sample.pdf',
  [string]$OutDir = 'C:\build\s4\gui',
  [string]$Log    = 'C:\build\s4\gui\viewer_test5.log'
)
# v4: Edge only. Drag-select + Ctrl+C with a long clipboard retry, plus UIA TextPattern read
#     of the PDF document (the accessibility/selection text layer).
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
}
public class Clip {
  [DllImport("user32.dll", SetLastError=true)] static extern bool OpenClipboard(IntPtr o);
  [DllImport("user32.dll", SetLastError=true)] static extern bool CloseClipboard();
  [DllImport("user32.dll", SetLastError=true)] static extern bool EmptyClipboard();
  [DllImport("user32.dll", SetLastError=true)] static extern IntPtr GetClipboardData(uint f);
  [DllImport("user32.dll", SetLastError=true)] static extern IntPtr SetClipboardData(uint f, IntPtr m);
  [DllImport("kernel32.dll", SetLastError=true)] static extern IntPtr GlobalAlloc(uint f, UIntPtr b);
  [DllImport("kernel32.dll", SetLastError=true)] static extern IntPtr GlobalLock(IntPtr m);
  [DllImport("kernel32.dll", SetLastError=true)] public static extern bool GlobalUnlock(IntPtr m);
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
      System.Threading.Thread.Sleep(500);
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
function Say([string]$m) { $m | Tee-Object -FilePath $Log -Append | Out-Null; Write-Output $m }
$cafe = 'caf' + [char]0x00E9
Say ("=== viewer_test v4 (Edge) " + (Get-Date -Format o))
function Force-Focus([IntPtr]$h) {
  [void][Win]::ShowWindow($h, 9)
  $fg = [Win]::GetForegroundWindow(); $fgPid = [uint32]0
  $fgTid = [Win]::GetWindowThreadProcessId($fg, [ref]$fgPid); $myTid = [Win]::GetCurrentThreadId()
  [void][Win]::AttachThreadInput($myTid, $fgTid, $true)
  [void][Win]::BringWindowToTop($h); [void][Win]::SetForegroundWindow($h)
  [void][Win]::AttachThreadInput($myTid, $fgTid, $false)
  Start-Sleep -Milliseconds 350
}
function Shot([IntPtr]$h, [string]$path) {
  $r = [Win+RECT]::new(); [void][Win]::GetWindowRect($h, [ref]$r)
  $bmp = New-Object System.Drawing.Bitmap ($r.Right - $r.Left), ($r.Bottom - $r.Top)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  try { $g.CopyFromScreen($r.Left, $r.Top, 0, 0, $bmp.Size) } catch { Say ("  shot err " + $_.Exception.Message) }
  $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png); $g.Dispose(); $bmp.Dispose(); Say ("  shot " + $path)
}

$exe = 'C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe'
$prof = Join-Path $OutDir 'profile-edge-v4'
if (Test-Path $prof) { Remove-Item -Recurse -Force $prof -ErrorAction SilentlyContinue }
$uri = 'file:///' + ($Pdf -replace '\\','/')
$p = Start-Process -FilePath $exe -ArgumentList @("--user-data-dir=$prof", '--no-first-run', '--no-default-browser-check',
      '--force-renderer-accessibility', '--disable-sync', '--no-service-autorun',
      '--disable-features=Translate,MediaRouter,msEdgeSignInPromo,msImplicitSignin',
      '--window-position=40,40', '--window-size=1300,950', '--new-window', $uri) -PassThru
$h = [IntPtr]::Zero
for ($i = 0; $i -lt 60; $i++) { Start-Sleep -Milliseconds 500; $p.Refresh(); if ($p.MainWindowHandle -ne [IntPtr]::Zero) { $h = $p.MainWindowHandle; break } }
Start-Sleep -Seconds 9
Force-Focus $h
Say ("  pid=" + $p.Id + " title='" + $p.MainWindowTitle + "'")

# --- UIA TextPattern read of the PDF document(s) ---
$root = [System.Windows.Automation.AutomationElement]::FromHandle($h)
$docs = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants,
        (New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty,
        [System.Windows.Automation.ControlType]::Document)))
Say ("  uia documents found: " + $docs.Count)
$uiaText = ''
foreach ($d in $docs) {
  try {
    $tp = $d.GetCurrentPattern([System.Windows.Automation.TextPattern]::Pattern)
    $t = $tp.DocumentRange.GetText(-1)
    if ($t.Length -gt $uiaText.Length) { $uiaText = $t }
  } catch { Say ("  textpattern failed: " + $_.Exception.Message) }
}
[IO.File]::WriteAllText((Join-Path $OutDir 'Edge-v4-uia-doc-text.txt'), $uiaText, (New-Object System.Text.UTF8Encoding $false))
$uesc = -join ($uiaText.Substring(0, [Math]::Min(400, $uiaText.Length)).ToCharArray() | ForEach-Object {
  if ([int]$_ -lt 32 -or [int]$_ -gt 126) { '[' + ([int]$_).ToString('X4') + ']' } else { $_ } })
Say ("  uia_doc_len=" + $uiaText.Length + " cafe=" + $uiaText.Contains($cafe) + " marker=" + $uiaText.Contains('MARKER-ALPHA') + " 0x1c=" + $uiaText.Contains([char]0x1C))
Say ("  uia_doc: " + $uesc)

# --- drag-select the first paragraph, then copy ---
$r = [Win+RECT]::new(); [void][Win]::GetWindowRect($h, [ref]$r)
Start-Sleep -Seconds 1
[System.Windows.Forms.SendKeys]::SendWait('{ESC}')
Start-Sleep -Milliseconds 500
[System.Windows.Forms.SendKeys]::SendWait('{ESC}')
Start-Sleep -Milliseconds 500
$y = $r.Top + 300
[void][Win]::SetCursorPos($r.Left + 300, $y); Start-Sleep -Milliseconds 400
[Win]::mouse_event(0x0002, 0, 0, 0, [IntPtr]::Zero); Start-Sleep -Milliseconds 200
for ($dx = 320; $dx -le 1050; $dx += 40) { [void][Win]::SetCursorPos($r.Left + $dx, $y); Start-Sleep -Milliseconds 60 }
[Win]::mouse_event(0x0004, 0, 0, 0, [IntPtr]::Zero); Start-Sleep -Milliseconds 800
Shot $h (Join-Path $OutDir 'Edge-v5-dragged.png')
[void][Clip]::SetText('SENTINEL-NOTHING-COPIED'); Start-Sleep -Milliseconds 300
[System.Windows.Forms.SendKeys]::SendWait('^c')
Start-Sleep -Seconds 4
$err = ''; $clip = [Clip]::GetText(10, [ref]$err)
if ($null -eq $clip) { $clip = '' }
[IO.File]::WriteAllText((Join-Path $OutDir 'Edge-v5-clipboard.txt'), $clip, (New-Object System.Text.UTF8Encoding $false))
$esc = -join ($clip.Substring(0, [Math]::Min(400, $clip.Length)).ToCharArray() | ForEach-Object {
  if ([int]$_ -lt 32 -or [int]$_ -gt 126) { '[' + ([int]$_).ToString('X4') + ']' } else { $_ } })
Say ("  drag_clip_len=" + $clip.Length + " err='" + $err + "' cafe=" + $clip.Contains($cafe) + " marker=" + $clip.Contains('MARKER-ALPHA') + " 0x1c=" + $clip.Contains([char]0x1C))
Say ("  drag_clip: " + $esc)

# --- Ctrl+A path, retried: a first-run sign-in bubble can steal the selection ---
$clip2 = ''
for ($attempt = 1; $attempt -le 4; $attempt++) {
  Say ("  ctrl+A attempt " + $attempt)
  Force-Focus $h
  [System.Windows.Forms.SendKeys]::SendWait('{ESC}'); Start-Sleep -Milliseconds 400
  [void][Win]::SetCursorPos($r.Left + 600, $r.Top + 500); Start-Sleep -Milliseconds 250
  [Win]::mouse_event(0x0002, 0, 0, 0, [IntPtr]::Zero); Start-Sleep -Milliseconds 120
  [Win]::mouse_event(0x0004, 0, 0, 0, [IntPtr]::Zero); Start-Sleep -Milliseconds 900
  [void][Clip]::SetText('SENTINEL-NOTHING-COPIED'); Start-Sleep -Milliseconds 300
  [System.Windows.Forms.SendKeys]::SendWait('^a'); Start-Sleep -Milliseconds 1500
  [System.Windows.Forms.SendKeys]::SendWait('^c'); Start-Sleep -Seconds 3
  $err2 = ''; $clip2 = [Clip]::GetText(10, [ref]$err2)
  if ($null -eq $clip2) { $clip2 = '' }
  Say ("    len=" + $clip2.Length + " has MARKER=" + $clip2.Contains('MARKER-ALPHA'))
  if ($clip2.Contains('MARKER-ALPHA')) { break }
  Start-Sleep -Seconds 2
}
[IO.File]::WriteAllText((Join-Path $OutDir 'Edge-v5-clipboardA.txt'), $clip2, (New-Object System.Text.UTF8Encoding $false))
Say ("  ctrla_clip_len=" + $clip2.Length + " cafe=" + $clip2.Contains($cafe) + " marker=" + $clip2.Contains('MARKER-ALPHA'))
Shot $h (Join-Path $OutDir 'Edge-v5-selectall.png')
try { taskkill /PID $p.Id /T /F | Out-Null } catch {}
Say "=== v4 done"
