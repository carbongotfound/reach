# Fixed action sequence in the Test Pad fixture, once per Speed mode, timed.
# Uses its own reach serve (port 7499, home tests\drv-home); moves the real pointer only
# inside the topmost Test Pad window and puts it back afterwards.
$ErrorActionPreference = 'Continue'
$root = (Resolve-Path "$PSScriptRoot\..").Path; $drvHome = "$root\tests\drv-home"; $outFile = "$root\tests\speed\bench.txt"
New-Item -ItemType Directory -Force $drvHome, "$root\tests\speed" | Out-Null
Add-Type -AssemblyName System.Windows.Forms
function J($o) { $o | ConvertTo-Json -Compress -Depth 5 }
function Run([string]$json) {
  try { (Invoke-WebRequest -UseBasicParsing -Method Post -Uri 'http://127.0.0.1:7499/run' -Headers @{ Authorization = "Bearer $script:tok" } -ContentType 'application/json' -Body ([Text.Encoding]::UTF8.GetBytes($json))).Content | ConvertFrom-Json }
  catch { $sr = New-Object IO.StreamReader($_.Exception.Response.GetResponseStream()); $sr.ReadToEnd() | ConvertFrom-Json }
}
$pad = Start-Process powershell -ArgumentList '-STA', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "$root\tests\testpad.ps1" -WindowStyle Hidden -PassThru
$lines = @("Speed bench $(Get-Date -Format s). Each row: ms per action (wall time of the HTTP call).", '')
$home0 = [System.Windows.Forms.Cursor]::Position
try {
  foreach ($mode in 'instant', 'fast', 'smooth') {
    "{`"port`":7499,`"autoPath`":false,`"agentCursor`":false,`"motion`":`"$mode`",`"pauseOnUser`":false}" | Set-Content -Encoding ascii "$drvHome\config.json"
    $env:REACH_HOME = $drvHome
    $drv = Start-Process "$root\target\release\reach.exe" -ArgumentList 'serve' -WindowStyle Hidden -PassThru
    Remove-Item Env:REACH_HOME
    Start-Sleep -Milliseconds 1200
    $tok = (Get-Content "$drvHome\token" -Raw).Trim()
    try {
      $padH = $null
      for ($i = 0; $i -lt 40 -and -not $padH; $i++) { Start-Sleep -Milliseconds 250; $padH = (((Run (J @{ tool = 'apps'; action = 'list'; filter = 'Test Pad'; json = $true })).text | ConvertFrom-Json) | Where-Object { $_.process -like 'powershell*' } | Select-Object -First 1).hwnd }
      $items = ((Run (J @{ tool = 'ui'; action = 'inspect'; window = $padH; json = $true })).text | ConvertFrom-Json).items
      $edit = $items | Where-Object { $_.name -eq 'Text editor' } | Select-Object -First 1
      $file = $items | Where-Object { $_.name -eq 'File' } | Select-Object -First 1
      Run (J @{ tool = 'ui'; action = 'set'; ref = 'Text editor'; value = ''; window = $padH }) | Out-Null
      $steps = [ordered]@{
        'move to File menu'      = @{ tool = 'mouse'; action = 'move'; coordinate = @($file.x, $file.y) }
        'click text box'         = @{ tool = 'mouse'; action = 'click'; coordinate = @($edit.x, $edit.y) }
        'type 37 chars'          = @{ tool = 'keyboard'; action = 'type'; text = 'Acme 1200, Northwind 850, Contoso 430' }
        'key ctrl+a'             = @{ tool = 'keyboard'; action = 'key'; text = 'ctrl+a' }
        'ui set (fast path)'     = @{ tool = 'ui'; action = 'set'; ref = 'Text editor'; value = 'Acme 1200'; window = $padH }
        'screenshot'             = @{ tool = 'screenshot'; window = $padH; force = $true }
        'screenshot, unchanged'  = @{ tool = 'screenshot'; window = $padH }
      }
      $row = @()
      foreach ($k in $steps.Keys) {
        $sw = [Diagnostics.Stopwatch]::StartNew(); $r = Run (J $steps[$k]); $sw.Stop()
        $row += "{0}={1}" -f $k, $sw.ElapsedMilliseconds
        if (-not $r.ok) { $row[-1] += " (failed: $($r.text))" }
      }
      $val = (Run (J @{ tool = 'ui'; action = 'read'; ref = 'Text editor'; window = $padH })).text
      $lines += ("{0,-8} " -f $mode) + ($row -join ' | ') + " | pad: $val"
    } finally { Stop-Process -Id $drv.Id -Force -ErrorAction SilentlyContinue }
  }
} finally {
  Stop-Process -Id $pad.Id -Force -ErrorAction SilentlyContinue
  [System.Windows.Forms.Cursor]::Position = $home0
}
$lines | Set-Content -Encoding utf8 $outFile
$lines
