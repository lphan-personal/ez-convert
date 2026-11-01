# screen.ps1 — capture the current desktop to a PNG and report what desktop we are on.
param([string]$Out = 'C:\build\s4\shots\screen.png')
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public class Desk {
  [DllImport("user32.dll", SetLastError=true)] public static extern IntPtr OpenInputDesktop(uint f, bool inherit, uint access);
  [DllImport("user32.dll", SetLastError=true)] public static extern bool CloseDesktop(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern int GetWindowTextW(IntPtr h, System.Text.StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
}
'@
$h = [Desk]::OpenInputDesktop(0, $false, 0x0001)
$deskName = 'UNKNOWN'
if ($h -ne [IntPtr]::Zero) { $deskName = 'OPENED_OK (unlocked interactive desktop)'; [void][Desk]::CloseDesktop($h) }
else { $deskName = 'FAILED err=' + [Runtime.InteropServices.Marshal]::GetLastWin32Error() + ' (locked / secure desktop)' }
$fg = [Desk]::GetForegroundWindow()
$sb = New-Object System.Text.StringBuilder 512
[void][Desk]::GetWindowTextW($fg, $sb, 512)
Write-Output ("desktop       : " + $deskName)
Write-Output ("foreground    : '" + $sb.ToString() + "'")
Write-Output ("session       : " + [Diagnostics.Process]::GetCurrentProcess().SessionId)
Write-Output ("user          : " + [Environment]::UserName)

$b = [System.Windows.Forms.SystemInformation]::VirtualScreen
Write-Output ("virtualscreen : " + $b.Width + "x" + $b.Height + " at " + $b.X + "," + $b.Y)
New-Item -ItemType Directory -Path (Split-Path $Out) -Force | Out-Null
# wake the display before grabbing
[void][Desk]::SetCursorPos($b.X + 40, $b.Y + 40)
Start-Sleep -Milliseconds 400
$bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($b.X, $b.Y, 0, 0, $bmp.Size)
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
Write-Output ("png           : " + $Out + " " + (Get-Item $Out).Length + " bytes")
