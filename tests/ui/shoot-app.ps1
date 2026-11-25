# Screenshot of the real app window (title bar included), captured from the window itself with PrintWindow,
# so nothing else on screen is read. Uses the test home in tests\app-home, never the user's history.
# Output: tests\ui\app-<label>.png
param([string]$Label = 'window', [int]$Width = 1200, [int]$Height = 780, [string]$Exe = "$PSScriptRoot\..\..\target\release\reach-app.exe", [switch]$Keep,
  # sets the app's Theme setting in the test home (system, light or dark); Windows' setting is untouched
  [ValidateSet('', 'system', 'light', 'dark')][string]$Theme = '',
  # 2 = the second monitor (the default: tests stay off the primary screen), 1 = the primary one
  [ValidateSet(1, 2)][int]$Monitor = 2,
  # another test home, e.g. a clean one from a demo run (tests\demo-home4), so shots show no old test tasks
  [string]$AppHome = "$PSScriptRoot\..\..\tests\app-home")
Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @'
using System; using System.Runtime.InteropServices; using System.Drawing;
public static class W {
  [StructLayout(LayoutKind.Sequential)] public struct R { public int L, T, Ri, B; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out R r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint f);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int cx, int cy, uint f);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
  public static Bitmap Grab(IntPtr h) {
    R r; GetWindowRect(h, out r);
    var b = new Bitmap(r.Ri - r.L, r.B - r.T);
    using (var g = Graphics.FromImage(b)) { var dc = g.GetHdc(); PrintWindow(h, dc, 2); g.ReleaseHdc(dc); }
    return b;
  }
}
'@
# per-monitor aware, so window rects and sizes are real pixels (PowerShell is DPI-unaware, and Windows
# would otherwise scale what we read and set, giving a cropped capture at 125 % and 150 %)
[W]::SetThreadDpiAwarenessContext([IntPtr]-4) | Out-Null
$env:REACH_HOME = $AppHome
# fence the Screens thumbnails to a test window (none open = blank screens), never the tester's real screens
if (-not $env:REACH_TEST_SCREEN) { $env:REACH_TEST_SCREEN = 'Test Pad' }
New-Item -ItemType Directory -Force $env:REACH_HOME | Out-Null
if ($Theme) {
  $cf = "$env:REACH_HOME\config.json"
  $c = if (Test-Path $cf) { Get-Content $cf -Raw | ConvertFrom-Json } else { [pscustomobject]@{} }
  $c | Add-Member -NotePropertyName theme -NotePropertyValue $Theme -Force
  $c | ConvertTo-Json -Compress | Set-Content -Encoding ascii $cf
}
# open straight on the chosen monitor, so nothing flashes on the other one first
Add-Type -AssemblyName System.Windows.Forms
$mon = [System.Windows.Forms.Screen]::AllScreens | Where-Object { $_.Primary -eq ($Monitor -eq 1) } | Select-Object -First 1
if (-not $mon) { throw "monitor $Monitor isn't connected; not opening the window anywhere else" }
$env:REACH_WINDOW_POS = "$($mon.WorkingArea.X + 40),$($mon.WorkingArea.Y + 40)"
$p = Start-Process $Exe -PassThru
Remove-Item Env:REACH_WINDOW_POS -ErrorAction SilentlyContinue
try {
  $h = [IntPtr]::Zero
  # the main window (titled Reach) shows once placed; until then the first window found can be the pill's overlay
  for ($i = 0; $i -lt 60 -and $h -eq [IntPtr]::Zero; $i++) { Start-Sleep -Milliseconds 250; $p.Refresh(); if ($p.MainWindowTitle -eq 'Reach') { $h = $p.MainWindowHandle } }
  if ($h -eq [IntPtr]::Zero) { throw "no window" }
  $k = [W]::GetDpiForWindow($h) / 96
  # on the second monitor (tests stay off the primary screen)
  Add-Type -AssemblyName System.Windows.Forms
  $m2 = [System.Windows.Forms.Screen]::AllScreens | Where-Object { $_.Primary -eq ($Monitor -eq 1) } | Select-Object -First 1
  if ($m2) { [W]::SetWindowPos($h, [IntPtr]::Zero, $m2.WorkingArea.X + 40, $m2.WorkingArea.Y + 40, 0, 0, 0x0015) | Out-Null; Start-Sleep -Milliseconds 300; $k = [W]::GetDpiForWindow($h) / 96 }
  [W]::SetWindowPos($h, [IntPtr]::Zero, 0, 0, [int]($Width * $k), [int]($Height * $k), 0x0016) | Out-Null  # NOMOVE|NOZORDER|NOACTIVATE
  Start-Sleep -Seconds 3
  $file = "$PSScriptRoot\..\..\tests\ui\app-$Label.png"
  ([W]::Grab($h)).Save($file, [System.Drawing.Imaging.ImageFormat]::Png)
  "saved $file (pid $($p.Id), dpi scale $k)"
} finally { if (-not $Keep) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue } }
