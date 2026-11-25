# Stand-in for the Codex CLI (REACH_CODEX=tests\fake-codex.cmd), for testing the app's ChatGPT
# brain without a ChatGPT sign-in. Speaks `codex exec --json` JSONL. Logs its arguments to
# tests\out\fake-codex-args.log. Prompt words: FAIL401 = a signed-out failure, SLOW = a 30 s turn.
$log = Join-Path $PSScriptRoot 'out\fake-codex-args.log'
New-Item -ItemType Directory -Force (Split-Path $log) | Out-Null
# fake-codex.cmd passes the arguments in FAKE_CODEX_ARGS (powershell -File would bind -c, -C as its own)
$a = @([regex]::Matches($env:FAKE_CODEX_ARGS, '"([^"]*)"|(\S+)') | ForEach-Object { if ($_.Groups[1].Success) { $_.Groups[1].Value } else { $_.Groups[2].Value } })
if ($a[0] -eq 'login') { 'Logged in using ChatGPT'; exit 0 }
($a -join ' | ') | Add-Content -Encoding utf8 $log
$prompt = [Console]::In.ReadToEnd()
function E($o) { [Console]::Out.WriteLine(($o | ConvertTo-Json -Compress -Depth 6)); [Console]::Out.Flush() }
$i = [array]::IndexOf($a, 'resume')
$thread = if ($i -ge 0) { $a[$i + 1] } else { 'fake-thread-1' }
E @{ type = 'thread.started'; thread_id = $thread }
E @{ type = 'turn.started' }
if ($prompt -match 'FAIL401') {
  E @{ type = 'error'; message = 'Reconnecting... 1/5 (unexpected status 401 Unauthorized)' }
  E @{ type = 'turn.failed'; error = @{ message = 'unexpected status 401 Unauthorized: Missing bearer or basic authentication in header' } }
  exit 1
}
if ($prompt -match 'SLOW') { Start-Sleep 30 }
if ($i -lt 0) {
  # one real tool call to the app, with the URL and token the app passed
  $url = ($a | Where-Object { $_ -like 'mcp_servers.reach.url=*' }) -replace '^mcp_servers.reach.url=', '' -replace '/mcp$', '/run'
  $r = try { Invoke-RestMethod -Method Post -Uri $url -Headers @{ Authorization = "Bearer $env:REACH_MCP_TOKEN" } -ContentType 'application/json' -Body '{"tool":"apps","action":"list"}' } catch { @{ ok = $false; text = "$_" } }
  E @{ type = 'item.completed'; item = @{ id = 'item_1'; type = 'mcp_tool_call'; server = 'reach'; tool = 'apps'; status = 'completed'; result = @{ content = @(@{ type = 'text'; text = "$($r.text)" }) } } }
  E @{ type = 'item.completed'; item = @{ id = 'item_2'; type = 'agent_message'; text = "Tool call ok=$($r.ok). Which file name should I use?" } }
} else {
  E @{ type = 'item.completed'; item = @{ id = 'item_3'; type = 'agent_message'; text = "Resumed $thread with: $($prompt.Trim())" } }
}
E @{ type = 'turn.completed'; usage = @{ input_tokens = 10; cached_input_tokens = 0; output_tokens = 5 } }
