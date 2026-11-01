param(
  [string]$Child = 'C:\build\s4\viewer_test5.ps1',
  [string]$Log   = 'C:\build\s4\gui\launcher.log'
)
# Runs as SYSTEM (via a scheduled task). Launches $Child as the interactive logged-on user
# in the active console session, so the harness can see and drive a real desktop.
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Path (Split-Path $Log) -Force | Out-Null
function Say([string]$m) { $m | Tee-Object -FilePath $Log -Append | Out-Null; Write-Output $m }

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public class Sess {
  [DllImport("kernel32.dll")] public static extern uint WTSGetActiveConsoleSessionId();
  [DllImport("wtsapi32.dll", SetLastError=true)] public static extern bool WTSQueryUserToken(uint sessionId, out IntPtr phToken);
  [DllImport("advapi32.dll", SetLastError=true)] public static extern bool DuplicateTokenEx(IntPtr hExistingToken, uint dwDesiredAccess, IntPtr lpTokenAttributes, int ImpersonationLevel, int TokenType, out IntPtr phNewToken);
  [DllImport("userenv.dll", SetLastError=true)] public static extern bool CreateEnvironmentBlock(out IntPtr lpEnvironment, IntPtr hToken, bool bInherit);
  [DllImport("userenv.dll")] public static extern bool DestroyEnvironmentBlock(IntPtr lpEnvironment);
  [DllImport("advapi32.dll", SetLastError=true, CharSet=CharSet.Unicode)] public static extern bool CreateProcessAsUser(IntPtr hToken, string lpApplicationName, string lpCommandLine, IntPtr lpProcessAttributes, IntPtr lpThreadAttributes, bool bInheritHandles, uint dwCreationFlags, IntPtr lpEnvironment, string lpCurrentDirectory, ref STARTUPINFO lpStartupInfo, out PROCESS_INFORMATION lpProcessInformation);
  [DllImport("advapi32.dll", SetLastError=true, CharSet=CharSet.Unicode)] public static extern bool CreateProcessWithTokenW(IntPtr hToken, uint dwLogonFlags, string lpApplicationName, string lpCommandLine, uint dwCreationFlags, IntPtr lpEnvironment, string lpCurrentDirectory, ref STARTUPINFO lpStartupInfo, out PROCESS_INFORMATION lpProcessInformation);
  [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] public struct STARTUPINFO {
    public int cb; public string lpReserved; public string lpDesktop; public string lpTitle;
    public int dwX, dwY, dwXSize, dwYSize, dwXCountChars, dwYCountChars, dwFillAttribute, dwFlags;
    public short wShowWindow, cbReserved2; public IntPtr lpReserved2, hStdInput, hStdOutput, hStdError; }
  [StructLayout(LayoutKind.Sequential)] public struct PROCESS_INFORMATION { public IntPtr hProcess, hThread; public int dwProcessId, dwThreadId; }
  [DllImport("kernel32.dll")] public static extern uint WaitForSingleObject(IntPtr h, uint ms);
  [DllImport("kernel32.dll")] public static extern bool GetExitCodeProcess(IntPtr h, out uint code);
  [DllImport("kernel32.dll")] public static extern bool CloseHandle(IntPtr h);
}
'@

$sid = [Sess]::WTSGetActiveConsoleSessionId()
Say ("launcher: active console session = " + $sid + "  (this process session=" + [Diagnostics.Process]::GetCurrentProcess().SessionId + ", user=" + [Environment]::UserName + ")")

$tok = [IntPtr]::Zero
if (-not [Sess]::WTSQueryUserToken($sid, [ref]$tok)) {
  Say ("WTSQueryUserToken FAILED err=" + [Runtime.InteropServices.Marshal]::GetLastWin32Error()); exit 10
}
$dup = [IntPtr]::Zero
# TOKEN_ALL_ACCESS 0xF01FF, SecurityImpersonation 2, TokenPrimary 1
if (-not [Sess]::DuplicateTokenEx($tok, 0xF01FF, [IntPtr]::Zero, 2, 1, [ref]$dup)) {
  Say ("DuplicateTokenEx FAILED err=" + [Runtime.InteropServices.Marshal]::GetLastWin32Error()); exit 11
}
$envBlk = [IntPtr]::Zero
if (-not [Sess]::CreateEnvironmentBlock([ref]$envBlk, $dup, $false)) { Say "CreateEnvironmentBlock failed (continuing with null env)" }

$psExe = 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe'
$cmd = '"' + $psExe + '" -NoProfile -ExecutionPolicy Bypass -File "' + $Child + '"'
$si = [Sess+STARTUPINFO]::new()
$si.cb = [Runtime.InteropServices.Marshal]::SizeOf($si)
$si.lpDesktop = 'winsta0\default'
$pi = [Sess+PROCESS_INFORMATION]::new()
# CREATE_UNICODE_ENVIRONMENT | CREATE_NO_WINDOW: without NO_WINDOW the child gets a console
# window that can hold the session's foreground, and then SendKeys/SetForegroundWindow on the
# real viewer silently do nothing (found on the win11-dev VM, 2026-11-01).
$flags = 0x00000400 -bor 0x08000000  # CREATE_UNICODE_ENVIRONMENT
$ok = [Sess]::CreateProcessAsUser($dup, $psExe, $cmd, [IntPtr]::Zero, [IntPtr]::Zero, $false, $flags, $envBlk, 'C:\build\s4', [ref]$si, [ref]$pi)
if (-not $ok) {
  $e1 = [Runtime.InteropServices.Marshal]::GetLastWin32Error()
  Say ("CreateProcessAsUser FAILED err=" + $e1 + " -> trying CreateProcessWithTokenW")
  $ok = [Sess]::CreateProcessWithTokenW($dup, 0, $psExe, $cmd, $flags, $envBlk, 'C:\build\s4', [ref]$si, [ref]$pi)
  if (-not $ok) { Say ("CreateProcessWithTokenW FAILED err=" + [Runtime.InteropServices.Marshal]::GetLastWin32Error()); exit 12 }
}
Say ("launcher: started pid=" + $pi.dwProcessId + " cmd=" + $cmd)
[void][Sess]::WaitForSingleObject($pi.hProcess, 900000)
$code = 0
[void][Sess]::GetExitCodeProcess($pi.hProcess, [ref]$code)
Say ("launcher: child exited code=" + $code)
[void][Sess]::CloseHandle($pi.hThread); [void][Sess]::CloseHandle($pi.hProcess)
if ($envBlk -ne [IntPtr]::Zero) { [void][Sess]::DestroyEnvironmentBlock($envBlk) }
