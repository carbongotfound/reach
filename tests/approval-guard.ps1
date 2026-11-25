# Engine-side approval guard, without relying on the model: starts the app with a trivial task
# (so a run is live), then sends risky actions straight to the app's own MCP server and answers
# the card through the app's buttons. Only the fake mail window (tests\testmail.ps1) is touched;
# its Send button only writes tests\out\testmail-sent.log. Output: tests\app\approval-guard.txt
param([switch]$Timeout)
$ErrorActionPreference = 'Continue'
$root = (Resolve-Path "$PSScriptRoot\..").Path; $out = "$root\tests\app"; $appHome = "$root\tests\app-home"; $drvHome = "$root\tests\drv-home"
$sentLog = "$root\tests\out\testmail-sent.log"; Remove-Item $sentLog -ErrorAction SilentlyContinue
Add-Type -AssemblyName System.Net.Http, System.Windows.Forms
$lines = @(); function Log($s) { $script:lines += $s; Write-Host $s }
function J($o) { $o | ConvertTo-Json -Compress -Depth 5 }
function Post($port, $tok, $json) {
  $c = New-Object System.Net.Http.HttpClient; $c.Timeout = [TimeSpan]::FromMinutes(5)
  $c.DefaultRequestHeaders.Authorization = New-Object System.Net.Http.Headers.AuthenticationHeaderValue('Bearer', $tok)
  $c.PostAsync("http://127.0.0.1:$port/run", (New-Object System.Net.Http.StringContent($json, [Text.Encoding]::UTF8, 'application/json')))
}
function Drv($o) { ((Post 7499 $script:dtok (J $o)).Result.Content.ReadAsStringAsync().Result) | ConvertFrom-Json }
function App($o) { Post $script:aport $script:atok (J $o) }
function Find($h, $q) { ((Drv @{ tool = 'ui'; action = 'find'; query = $q; json = $true; window = $h }).text | ConvertFrom-Json).items }
function Sent { if (Test-Path $sentLog) { @(Get-Content $sentLog).Count } else { 0 } }
function Events { @(Get-Content "$($script:runDir.FullName)\ui.jsonl" -Encoding UTF8 | ForEach-Object { try { $_ | ConvertFrom-Json } catch {} }) }  # the last line may be half written

# One risky action: send it, wait for the card, press $answer ('' = don't answer), return the result.
function Case($label, $action, $answer) {
  $n0 = @(Events | Where-Object type -eq 'approval').Count; $s0 = Sent
  $t0 = Get-Date; $task = App $action; $card = $null; $b = $null
  while (-not $task.IsCompleted -and ((Get-Date) - $t0).TotalSeconds -lt 150) {
    Start-Sleep -Milliseconds 200
    if (-not $card) {
      $card = @(Events | Where-Object type -eq 'approval')[$n0]
      if ($card -and $answer) {
        $sw = [Diagnostics.Stopwatch]::StartNew()
        # the log line is written just before the card is drawn, so look for the button a few times
        for ($k = 0; $k -lt 20 -and -not $b; $k++) { $b = Find $script:appH $answer | Where-Object { $_.type -eq 'button' -and $_.name -eq $answer } | Select-Object -First 1; if (-not $b) { Start-Sleep -Milliseconds 150 } }
        $pr = Drv @{ tool = 'ui'; action = 'invoke'; ref = $b.ref }
        Log ("   pressed {0} ({1}): {2} in {3} ms" -f $answer, $b.ref, ($pr.text -split "`n")[0], $sw.ElapsedMilliseconds)
      }
    }
  }
  $r = $task.Result.Content.ReadAsStringAsync().Result | ConvertFrom-Json
  $ans = Events | Where-Object { $_.type -eq 'approval-answer' -and $card -and $_.id -eq $card.id } | Select-Object -First 1
  Log ("{0}: card={1} answer={2} after {3:n1}s | ok={4} {5} | sent {6} -> {7}" -f $label, $(if ($card) { "$($card.by) '$($card.action)' in '$($card.where)'" } else { 'none' }),
    $(if ($ans) { $ans.decision } else { '-' }), ((Get-Date) - $t0).TotalSeconds, $r.ok, ($r.text -split "`n")[0], $s0, (Sent))
}

$env:REACH_HOME = $drvHome; '{"port":7499,"autoPath":false,"agentCursor":false}' | Set-Content -Encoding ascii "$drvHome\config.json"
$drv = Start-Process "$root\target\release\reach.exe" -ArgumentList 'serve' -WindowStyle Hidden -PassThru; Remove-Item Env:REACH_HOME
$mail = Start-Process powershell -ArgumentList '-STA', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "$root\tests\testmail.ps1" -WindowStyle Hidden -PassThru
$env:REACH_HOME = $appHome; $env:REACH_TEST_SCREEN = 'Reach Test Mail'
$app = Start-Process "$root\target\release\reach-app.exe" -PassThru
Remove-Item Env:REACH_HOME, Env:REACH_TEST_SCREEN
$home0 = [System.Windows.Forms.Cursor]::Position
try {
  Start-Sleep 2; $dtok = (Get-Content "$drvHome\token" -Raw).Trim()
  $mailH = $null; $appH = $null
  for ($i = 0; $i -lt 60 -and -not ($mailH -and $appH); $i++) {
    Start-Sleep -Milliseconds 300
    $l = (Drv @{ tool = 'apps'; action = 'list'; json = $true }).text | ConvertFrom-Json
    $mailH = ($l | Where-Object { $_.title -eq 'Reach Test Mail' } | Select-Object -First 1).hwnd
    $appH = ($l | Where-Object { $_.process -eq 'reach-app' } | Select-Object -First 1).hwnd
  }
  for ($i = 0; $i -lt 40; $i++) { if (Find $appH 'Describe a task' | Where-Object { $_.enabled }) { break }; Start-Sleep -Milliseconds 300 }
  # a live run: the app only accepts tool calls while one is running or waiting
  $box = Find $appH 'Describe a task' | Select-Object -First 1
  Drv @{ tool = 'ui'; action = 'set'; ref = $box.ref; value = 'Reply with the single word ready. Do not use any tools.' } | Out-Null
  $before = @(Get-ChildItem "$appHome\runs" -Directory).Name
  $start = Find $appH 'Start' | Where-Object { $_.type -eq 'button' } | Select-Object -First 1
  Drv @{ tool = 'ui'; action = 'invoke'; ref = $start.ref } | Out-Null
  for ($i = 0; $i -lt 50 -and -not $runDir; $i++) { Start-Sleep -Milliseconds 200; $runDir = Get-ChildItem "$appHome\runs" -Directory | Where-Object { $before -notcontains $_.Name } | Select-Object -First 1 }
  for ($i = 0; $i -lt 120; $i++) { if (Events | Where-Object { $_.type -eq 'state' -and $_.state -eq 'waiting' }) { break }; Start-Sleep -Milliseconds 500 }
  $mcp = Get-Content "$appHome\mcp.json" -Raw | ConvertFrom-Json
  $aport = ([Uri]$mcp.mcpServers.reach.url).Port; $atok = $mcp.mcpServers.reach.headers.Authorization -replace '^Bearer ', ''
  Log "run $($runDir.Name) is waiting; app MCP port $aport"

  $send = @{ tool = 'ui'; action = 'invoke'; ref = 'Send'; window = $mailH }
  Case 'A. ui invoke Send, Deny' $send 'Deny'
  App @{ tool = 'apps'; action = 'focus'; window = $mailH } | Out-Null
  App @{ tool = 'ui'; action = 'focus'; ref = 'Send'; window = $mailH } | Out-Null
  Case 'B. Enter on focused Send, Deny' @{ tool = 'keyboard'; action = 'key'; text = 'Return' } 'Deny'
  $it = (((App @{ tool = 'ui'; action = 'find'; query = 'Send'; json = $true; window = $mailH }).Result.Content.ReadAsStringAsync().Result | ConvertFrom-Json).text | ConvertFrom-Json).items | Where-Object { $_.type -eq 'button' } | Select-Object -First 1
  Case 'C. mouse click on Send, Deny' @{ tool = 'mouse'; action = 'click'; coordinate = @($it.x, $it.y) } 'Deny'
  Case 'D. ui invoke Save draft (not risky)' @{ tool = 'ui'; action = 'invoke'; ref = 'Save draft'; window = $mailH } ''
  Case 'E. ui invoke Send, Approve' $send 'Approve'
  Case 'F. request_approval, Approve' @{ tool = 'request_approval'; action = 'Send email'; where = 'Reach Test Mail'; content = 'test'; why = 'guard test'; undo = 'no' } 'Approve'
  Case 'G. ui invoke Send right after F (covered by that approval)' $send ''
  Case 'H. ui invoke Send again (approval used up), Deny' $send 'Deny'
  if ($Timeout) { Case 'I. ui invoke Send, no answer' $send '' }
  # Pause-on-user: simulated physical input, then a harmless key (Shift) has to wait for quiet.
  App @{ tool = 'apps'; action = 'focus'; window = $mailH } | Out-Null
  $p0 = @(Events | Where-Object type -eq 'pause').Count
  App @{ tool = 'pause_on_user'; action = 'simulate'; value = 3000 } | Out-Null
  $sw = [Diagnostics.Stopwatch]::StartNew(); $r = (App @{ tool = 'keyboard'; action = 'key'; text = 'shift' }).Result.Content.ReadAsStringAsync().Result | ConvertFrom-Json
  $pe = @(Events | Where-Object type -eq 'pause')[$p0..99] | ForEach-Object { "paused=$($_.paused)" }
  Log ("J. key while you're active (3 s): waited {0} ms, ok={1}, pause events: {2}" -f $sw.ElapsedMilliseconds, $r.ok, ($pe -join ', '))
  App @{ tool = 'pause_on_user'; action = 'simulate'; value = 20000 } | Out-Null
  $sw = [Diagnostics.Stopwatch]::StartNew(); $task = App @{ tool = 'keyboard'; action = 'key'; text = 'shift' }
  Start-Sleep -Milliseconds 1500
  $b = $null; for ($k = 0; $k -lt 20 -and -not $b; $k++) { $b = Find $appH 'Resume' | Where-Object { $_.type -eq 'button' } | Select-Object -First 1; if (-not $b) { Start-Sleep -Milliseconds 150 } }
  Drv @{ tool = 'ui'; action = 'invoke'; ref = $b.ref } | Out-Null
  $r = $task.Result.Content.ReadAsStringAsync().Result | ConvertFrom-Json
  Log ("K. key while you're active (20 s), Resume pressed after 1.5 s: done in {0} ms, ok={1}, Resume button found: {2}" -f $sw.ElapsedMilliseconds, $r.ok, [bool]$b)
  Log "fake mail sent log: $(if (Test-Path $sentLog) { (Get-Content $sentLog) -join ' || ' } else { 'none' })"
  $shot = Drv @{ tool = 'screenshot'; window = $mailH; force = $true }; if ($shot.path) { Copy-Item $shot.path "$out\approval-guard-mail.jpg" -Force }
} finally {
  [System.Windows.Forms.Cursor]::Position = $home0
  foreach ($p in $app, $mail, $drv) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }
  $lines | Set-Content -Encoding utf8 "$out\approval-guard.txt"
}
