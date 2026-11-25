# Screenshots of every app screen (design preview with a mocked backend), light and dark, at 100 / 125 / 150 %.
# Needs the preview server: cd app; npx vite --port 5199 --strictPort
# Output: <OutDir>\<scenario>-<theme>[-125|-150].png  (default OutDir: tests\ui)
param([string[]]$Only, [string]$OutDir = $PSScriptRoot, [switch]$Quick)
$E = "C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"
New-Item -ItemType Directory -Force $OutDir | Out-Null
$prof = Join-Path $env:TEMP "reach-ui-shoot-profile"
$scen = 'start','start-menu','start-delete','start-empty','start-loading','signed-out','codex-signed-out','running-early','running','paused','held','waiting','done','failed','long',
        'approval','approval-engine','approval-denied','codex-approval','settings','settings-loading','settings-error'
if ($Only) { $scen = $Only }
function Shoot($s, $theme, $w, $h, $dsf, $suffix) {
  $file = Join-Path $OutDir "$s-$theme$suffix.png"
  $scheme = if ($theme -eq 'dark') { 0 } else { 1 }
  # window size is logical px; the image comes out at size x dsf
  $a = @('--headless=new', '--disable-gpu', '--hide-scrollbars', "--user-data-dir=$prof", "--window-size=$w,$h",
         "--force-device-scale-factor=$dsf", "--blink-settings=preferredColorScheme=$scheme", '--virtual-time-budget=5000', '--force-prefers-reduced-motion',
         "--screenshot=$file", "http://localhost:5199/preview.html?s=$s")
  if ($theme -eq 'dark') { $a += '--force-dark-mode' }
  $p = Start-Process $E -ArgumentList $a -PassThru -WindowStyle Hidden
  if (-not $p.WaitForExit(60000)) { Stop-Process -Id $p.Id -Force }
}
foreach ($s in $scen) { foreach ($t in 'light','dark') { Shoot $s $t 1200 780 1 '' } }
if (-not $Quick) {
  # 125 % at the default window size, 150 % at the window minimum (900 x 600 logical), and a wide 1600 x 1000 window
  foreach ($s in 'start','running','approval','done','settings') { foreach ($t in 'light','dark') { Shoot $s $t 1200 780 1.25 '-125' } }
  foreach ($s in 'start','running','approval','done','settings') { foreach ($t in 'light','dark') { Shoot $s $t 900 600 1.5 '-150' } }
  foreach ($s in 'start','running','approval','done') { foreach ($t in 'light','dark') { Shoot $s $t 1600 1000 1 '-1600' } }
}
Get-ChildItem $OutDir -Filter *.png | Measure-Object | ForEach-Object { "$($_.Count) screenshots in $OutDir" }
