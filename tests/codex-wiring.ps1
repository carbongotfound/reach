# Codex (ChatGPT) brain wiring, without a ChatGPT sign-in: starts the app with the ChatGPT brain
# picked, reads the picker state and the signed-out note, then runs `codex exec` with the app's
# arguments against the app's MCP server and checks Codex connected to it and listed its tools.
# Output: tests\app\codex-wiring.txt
$ErrorActionPreference = 'Continue'
$root = (Resolve-Path "$PSScriptRoot\..").Path; $out = "$root\tests\app"; $appHome = "$root\tests\app-home"; $drvHome = "$root\tests\drv-home"
Add-Type -AssemblyName System.Net.Http
$lines = @(); function Log($s) { $script:lines += $s; Write-Host $s }
function J($o) { $o | ConvertTo-Json -Compress -Depth 5 }
function Drv($o) {
  $c = New-Object System.Net.Http.HttpClient
  $c.DefaultRequestHeaders.Authorization = New-Object System.Net.Http.Headers.AuthenticationHeaderValue('Bearer', $script:dtok)
  ($c.PostAsync('http://127.0.0.1:7499/run', (New-Object System.Net.Http.StringContent((J $o), [Text.Encoding]::UTF8, 'application/json'))).Result.Content.ReadAsStringAsync().Result) | ConvertFrom-Json
}
function Find($h, $q) { ((Drv @{ tool = 'ui'; action = 'find'; query = $q; json = $true; window = $h }).text | ConvertFrom-Json).items }

$brainFile = "$appHome\brain.txt"; $brain0 = if (Test-Path $brainFile) { Get-Content $brainFile -Raw } else { $null }
'codex' | Set-Content -Encoding ascii -NoNewline $brainFile
$env:REACH_HOME = $drvHome; '{"port":7499,"autoPath":false,"agentCursor":false}' | Set-Content -Encoding ascii "$drvHome\config.json"
$drv = Start-Process "$root\target\release\reach.exe" -ArgumentList 'serve' -WindowStyle Hidden -PassThru; Remove-Item Env:REACH_HOME
$env:REACH_HOME = $appHome; $env:REACH_TEST_SCREEN = 'Reach'
$app = Start-Process "$root\target\release\reach-app.exe" -PassThru
Remove-Item Env:REACH_HOME, Env:REACH_TEST_SCREEN
$cx = $null
try {
  Start-Sleep 2; $dtok = (Get-Content "$drvHome\token" -Raw).Trim()
  $appH = $null
  for ($i = 0; $i -lt 60 -and -not $appH; $i++) {
    Start-Sleep -Milliseconds 300
    $appH = (((Drv @{ tool = 'apps'; action = 'list'; json = $true }).text | ConvertFrom-Json) | Where-Object { $_.process -eq 'reach-app' } | Select-Object -First 1).hwnd
  }
  $note = $null
  for ($i = 0; $i -lt 80 -and -not $note; $i++) { Start-Sleep -Milliseconds 300; $note = Find $appH 'signed in' | Select-Object -First 1 }
  $radios = Find $appH 'ChatGPT' | Where-Object { $_.type -eq 'radiobutton' -or $_.type -eq 'button' }
  Log "1. picker: $(($radios | ForEach-Object { "$($_.type) '$($_.name)'" }) -join ', ')"
  Log "2. signed-out note: $(if ($note) { $note.name } else { 'not found' })"
  $start = Find $appH 'Start' | Where-Object { $_.type -eq 'button' -and $_.name -like 'Start*' } | Select-Object -First 1
  Log "3. Start button: $($start | ConvertTo-Json -Compress)"

  # 4. Codex with the app's arguments (as in brain.rs codex_cmd), against the app's MCP server.
  $mcp = Get-Content "$appHome\mcp.json" -Raw -ErrorAction SilentlyContinue | ConvertFrom-Json
  for ($i = 0; $i -lt 20 -and -not $mcp; $i++) { Start-Sleep -Milliseconds 300; $mcp = Get-Content "$appHome\mcp.json" -Raw -ErrorAction SilentlyContinue | ConvertFrom-Json }
  $port = ([Uri]$mcp.mcpServers.reach.url).Port; $tok = $mcp.mcpServers.reach.headers.Authorization -replace '^Bearer ', ''
  $work = "$appHome\work"; New-Item -ItemType Directory -Force $work | Out-Null
  $sys = "$out\codex-wiring-system.md"; 'You are a test. Reply with the word ready.' | Set-Content -Encoding utf8 $sys
  $a = @('exec', '--json', '--skip-git-repo-check', '--ignore-user-config', '--ignore-rules', '-s', 'read-only', '-C', $work,
         '-c', 'approval_policy=never', '-c', 'web_search=disabled', '-c', "model_instructions_file=$sys",
         '-c', "mcp_servers.reach.url=http://127.0.0.1:$port/mcp", '-c', 'mcp_servers.reach.bearer_token_env_var=REACH_MCP_TOKEN',
         '-c', 'mcp_servers.reach.tool_timeout_sec=600', '-c', 'mcp_servers.reach.required=true')
  foreach ($f in 'shell_tool','unified_exec','computer_use','browser_use','browser_use_external','in_app_browser','apps','plugins','image_generation','multi_agent','goals','memories','skill_mcp_dependency_install','tool_suggest') { $a += '--disable', $f }
  $a += '-'
  foreach ($case in @(@{ n = 'right token'; t = $tok }, @{ n = 'wrong token'; t = 'x' })) {
    $env:REACH_MCP_TOKEN = $case.t; $env:RUST_LOG = 'codex_core=info,codex_rmcp_client=info,rmcp=info'
    $psi = New-Object Diagnostics.ProcessStartInfo 'cmd.exe', ('/c codex ' + (($a | ForEach-Object { if ($_ -match '\s') { "`"$_`"" } else { $_ } }) -join ' '))
    $psi.UseShellExecute = $false; $psi.RedirectStandardInput = $true; $psi.RedirectStandardOutput = $true; $psi.RedirectStandardError = $true; $psi.CreateNoWindow = $true
    $cx = [Diagnostics.Process]::Start($psi)
    $cx.StandardInput.Write('Reply with the word ready.'); $cx.StandardInput.Close()
    $so = $cx.StandardOutput.ReadToEndAsync(); $se = $cx.StandardError.ReadToEndAsync()
    if (-not $cx.WaitForExit(45000)) { & taskkill /PID $cx.Id /T /F | Out-Null }
    Remove-Item Env:REACH_MCP_TOKEN, Env:RUST_LOG
    $all = $so.Result + "`n" + $se.Result
    $all | Set-Content -Encoding utf8 "$out\codex-wiring-$($case.n -replace ' ', '-').log"
    $mcpLines = ($all -split "`n") | Where-Object { $_ -match 'mcp|reach' -and $_ -notmatch 'Reading additional' } | Select-Object -First 6
    Log "4. codex exec, $($case.n): exit $($cx.ExitCode)"
    $mcpLines | ForEach-Object { Log "     $($_.Trim())" }
    $ev = ($so.Result -split "`n") | Where-Object { $_ -match '^\{' } | ForEach-Object { try { ($_ | ConvertFrom-Json).type } catch {} }
    Log "     json events: $(($ev | Select-Object -Unique) -join ', ')"
  }
} finally {
  foreach ($p in $app, $drv) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }
  if ($null -eq $brain0) { Remove-Item $brainFile -ErrorAction SilentlyContinue } else { $brain0 | Set-Content -Encoding ascii -NoNewline $brainFile }
  $lines | Set-Content -Encoding utf8 "$out\codex-wiring.txt"
}
