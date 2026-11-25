# The app's ChatGPT brain end to end, with tests\fake-codex.cmd standing in for the Codex CLI
# (no ChatGPT sign-in needed). Drives the app's own window only. Checks: run events and the
# agent name, one tool call through the app's server, a follow-up via `resume`, the signed-out
# failure message, and Stop during a turn. Output: tests\app\codex-fake-e2e.txt
$ErrorActionPreference = 'Continue'
$root = (Resolve-Path "$PSScriptRoot\..").Path; $out = "$root\tests\app"; $appHome = "$root\tests\app-home"; $drvHome = "$root\tests\drv-home"
$argLog = "$root\tests\out\fake-codex-args.log"; Remove-Item $argLog -ErrorAction SilentlyContinue
Add-Type -AssemblyName System.Net.Http
$lines = @(); function Log($s) { $script:lines += $s; Write-Host $s }
function J($o) { $o | ConvertTo-Json -Compress -Depth 5 }
function Drv($o) {
  $c = New-Object System.Net.Http.HttpClient
  $c.DefaultRequestHeaders.Authorization = New-Object System.Net.Http.Headers.AuthenticationHeaderValue('Bearer', $script:dtok)
  ($c.PostAsync('http://127.0.0.1:7499/run', (New-Object System.Net.Http.StringContent((J $o), [Text.Encoding]::UTF8, 'application/json'))).Result.Content.ReadAsStringAsync().Result) | ConvertFrom-Json
}
function Find($q) { ((Drv @{ tool = 'ui'; action = 'find'; query = $q; json = $true; window = $script:appH }).text | ConvertFrom-Json).items }
function Btn($name) { for ($k = 0; $k -lt 20; $k++) { $b = Find $name | Where-Object { $_.type -eq 'button' -and $_.name -like "$name*" -and $_.enabled } | Select-Object -First 1; if ($b) { return $b }; Start-Sleep -Milliseconds 200 } }
function Press($name) { $b = Btn $name; if ($b) { Drv @{ tool = 'ui'; action = 'invoke'; ref = $b.ref } | Out-Null } else { Log "   (no enabled '$name' button)" } }
function Set-Text($label, $text) { $t = Find $label | Select-Object -First 1; Drv @{ tool = 'ui'; action = 'set'; ref = $t.ref; value = $text } | Out-Null }
function Events($d) { @(Get-Content "$d\ui.jsonl" -Encoding UTF8 -ErrorAction SilentlyContinue | ForEach-Object { try { $_ | ConvertFrom-Json } catch {} }) }
function WaitState($d, $states, $sec) { $t0 = Get-Date; while (((Get-Date) - $t0).TotalSeconds -lt $sec) { $s = (Events $d | Where-Object type -eq 'state' | Select-Object -Last 1).state; if ($states -contains $s) { return $s }; Start-Sleep -Milliseconds 300 }; "timeout($s)" }
function NewRun($task) {
  $before = @(Get-ChildItem "$appHome\runs" -Directory).Name
  Set-Text 'Describe a task' $task; Press 'Send'
  for ($i = 0; $i -lt 50; $i++) { Start-Sleep -Milliseconds 200; $d = Get-ChildItem "$appHome\runs" -Directory | Where-Object { $before -notcontains $_.Name } | Select-Object -First 1; if ($d) { return $d.FullName } }
}
function Summary($d) { Events $d | Where-Object { $_.type -in 'start', 'step', 'text', 'status', 'state', 'user' } | ForEach-Object { $e = $_
  switch ($e.type) { 'start' { "     start agent=$($e.agent)" } 'step' { "     step $($e.n) $($e.tool) $($e.action) ok=$($e.ok)" } 'state' { "     state $($e.state)" } default { "     $($e.type): $($e.text)" } } } }

$brainFile = "$appHome\brain.txt"; $brain0 = if (Test-Path $brainFile) { Get-Content $brainFile -Raw } else { $null }
'codex' | Set-Content -Encoding ascii -NoNewline $brainFile
$env:REACH_HOME = $drvHome; '{"port":7499,"autoPath":false,"agentCursor":false}' | Set-Content -Encoding ascii "$drvHome\config.json"
$drv = Start-Process "$root\target\release\reach.exe" -ArgumentList 'serve' -WindowStyle Hidden -PassThru; Remove-Item Env:REACH_HOME
$env:REACH_HOME = $appHome; $env:REACH_TEST_SCREEN = 'Reach'; $env:REACH_CODEX = "$root\tests\fake-codex.cmd"
$app = Start-Process "$root\target\release\reach-app.exe" -PassThru
Remove-Item Env:REACH_HOME, Env:REACH_TEST_SCREEN, Env:REACH_CODEX
try {
  Start-Sleep 2; $dtok = (Get-Content "$drvHome\token" -Raw).Trim()
  for ($i = 0; $i -lt 60 -and -not $appH; $i++) { Start-Sleep -Milliseconds 300; $appH = (((Drv @{ tool = 'apps'; action = 'list'; json = $true }).text | ConvertFrom-Json) | Where-Object { $_.process -eq 'reach-app' } | Select-Object -First 1).hwnd }
  for ($i = 0; $i -lt 40; $i++) { if (-not (Find 'signed in')) { if ($i -gt 3) { break } }; Start-Sleep -Milliseconds 300 }

  $d = NewRun 'Make a test file'
  Log "A. task, then a question: $(WaitState $d 'waiting','failed' 20) ($(Split-Path $d -Leaf))"; Summary $d | ForEach-Object { Log $_ }
  Set-Text 'Reply to the agent' 'invoices'; Press 'Send'
  Start-Sleep 1; Log "B. reply via resume: $(WaitState $d 'waiting','failed' 20)"
  Events $d | Where-Object { $_.type -in 'user', 'text' } | Select-Object -Last 2 | ForEach-Object { Log "     $($_.type): $($_.text)" }

  Press 'Stop'; Log "   Stop on the waiting run: $(WaitState $d 'stopped' 10)"
  Press 'New task'; Start-Sleep -Milliseconds 400
  $d = NewRun 'FAIL401 test'
  Log "C. signed-out failure: $(WaitState $d 'failed', 'done' 20)"
  Events $d | Where-Object type -eq 'status' | ForEach-Object { Log "     [$($_.level)] $($_.text)" }

  Press 'New task'; Start-Sleep -Milliseconds 400
  $d = NewRun 'SLOW test'
  Start-Sleep 3; $sw = [Diagnostics.Stopwatch]::StartNew(); Press 'Stop'
  Log "D. Stop during a turn: $(WaitState $d 'stopped', 'failed' 15) in $($sw.ElapsedMilliseconds) ms"
  Events $d | Where-Object type -eq 'status' | ForEach-Object { Log "     [$($_.level)] $($_.text)" }
  $left = Get-CimInstance Win32_Process -Filter "Name='powershell.exe'" | Where-Object { $_.CommandLine -like '*fake-codex.ps1*' }
  Log "     fake codex processes left: $(@($left).Count)"

  Log "fake codex calls:"; Get-Content $argLog | ForEach-Object { $a = $_ -split ' \| '; Log ("     resume={0} disabled={1} url={2}" -f ($a -contains 'resume'), (@($a | Where-Object { $_ -in 'shell_tool','computer_use','browser_use','apps','plugins' }).Count), ($a | Where-Object { $_ -like 'mcp_servers.reach.url=*' })) }
} finally {
  foreach ($p in $app, $drv) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }
  if ($null -eq $brain0) { Remove-Item $brainFile -ErrorAction SilentlyContinue } else { $brain0 | Set-Content -Encoding ascii -NoNewline $brainFile }
  $lines | Set-Content -Encoding utf8 "$out\codex-fake-e2e.txt"
}
