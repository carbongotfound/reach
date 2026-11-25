# The real app window at 125 % and 150 % on both monitors (tests\ui\app-dpi<scale>-m<monitor>-<theme>-<width>.png).
# Notes the original scales first (tests\dpi\restore-app.txt) and always puts them back.
# Changes Windows display scaling while it runs (and restores it); run it when nobody is using the PC.
param([string]$SetDpi = "$PSScriptRoot\..\..\target\debug\examples\setdpi.exe", [string]$Exe = "$PSScriptRoot\..\..\target\release\reach-app.exe")
Add-Type -AssemblyName System.Windows.Forms
$list = & $SetDpi list
$list | Set-Content "$PSScriptRoot\..\..\tests\dpi\restore-app.txt"
$mons = foreach ($l in $list) {
  $p = $l.Split(' ')
  $scr = [System.Windows.Forms.Screen]::AllScreens | Where-Object { $_.DeviceName -eq $p[1] }
  [pscustomobject]@{ idx = [int]$p[0]; orig = [int]$p[5]; primary = $scr.Primary }
}
$log = @("original: $($list -join ' | ')")
try {
  foreach ($scale in 125, 150) {
    foreach ($m in $mons) { & $SetDpi set $m.idx $scale | Out-Null }
    Start-Sleep 3
    foreach ($mon in 2, 1) {
      # the minimum window (900 x 560) and the largest that fits a 1920 x 1080 screen at this scale
      $big = if ($scale -eq 150) { 1240, 660 } else { 1200, 780 }
      foreach ($size in @(900, 560), $big) {
        foreach ($theme in 'light', 'dark') {
          $r = & "$PSScriptRoot\shoot-app.ps1" -Label "dpi$scale-m$mon-$theme-$($size[0])" -Monitor $mon -Theme $theme -Exe $Exe -Width $size[0] -Height $size[1]
          $log += "$scale% monitor $mon $($size -join 'x') ${theme}: $r"
        }
      }
    }
  }
} finally {
  foreach ($m in $mons) { & $SetDpi set $m.idx $m.orig | Out-Null }
  $log += "restored: $((& $SetDpi list) -join ' | ')"
  $log | Set-Content "$PSScriptRoot\..\..\tests\dpi\app-report.txt"
  $log
}
