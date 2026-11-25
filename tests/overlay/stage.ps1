# Real-display captures of the agent cursor and pill: waits for the PC gate, then runs the stage
# harness on a second monitor, never the primary one.
#   tests\overlay\stage.ps1 -Label r10
param([string]$Label = 'x', [switch]$NoGate)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
$m2 = [System.Windows.Forms.Screen]::AllScreens | Where-Object { -not $_.Primary } | Select-Object -First 1
if (-not $m2) { throw 'no second monitor' }
if (-not $NoGate -and (Test-Path "$PSScriptRoot\..\pc-gate.ps1")) { & "$PSScriptRoot\..\pc-gate.ps1" -Wait | Out-Null }  # optional local gate
$exe = "$PSScriptRoot\..\..\target\release\examples\cursor_stage.exe"
$bmps = @("$PSScriptRoot\stage\photo.bmp", "$PSScriptRoot\stage\busy.bmp", "$PSScriptRoot\stage\busy-dark.bmp")
$out = "$PSScriptRoot\stage\$Label"
$p = Start-Process $exe -ArgumentList (@($out, ($m2.WorkingArea.X + 200), ($m2.WorkingArea.Y + 200)) + $bmps) -PassThru -NoNewWindow
if (-not $p.WaitForExit(180000)) { Stop-Process -Id $p.Id -Force; 'timeout' } else { "done -> $out" }
