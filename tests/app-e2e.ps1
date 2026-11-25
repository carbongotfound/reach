# End-to-end test of the Reach app. Starts the app (isolated home tests\app-home), a
# Test Pad window, and a separate Reach server (port 7499) used only to drive the app's
# own window. Types the task into the app, presses Start, follows runs\<id>\ui.jsonl.
# -StopAfter N presses Stop (button or hotkey) once N steps have run, then checks the
# halt time and that Test Pad stops changing. Only processes started here are stopped.
param(
  [string]$Task = 'In the window titled "Untitled - Test Pad", write hello',
  [int]$StopAfter = 0,
  [int]$TakeOverAfter = 0,
  [ValidateSet('button', 'hotkey')][string]$StopBy = 'button',
  [int]$TimeoutSec = 240,
  [string]$Name = 'run',
  # notepad = real Windows Notepad (allowed by the user 2026-11-24; it restores their saved tabs). Refuses to run if Notepad is already open.
  # calculator = the harness doesn't open anything: the agent must open Calculator itself. Refuses if Calculator is already open.
  # realapps = real apps opened here on the second monitor (-With: notepad = a private copy of classic Notepad, no saved
  # tabs; calculator; paint; charmap; code = VS Code with a temporary profile on an empty folder; edge / todo = a separate InPrivate
  # Edge with its own temporary profile on tests\realapps\recipe.html / todo.html).
  # The fence lets the agent see and touch only those windows (REACH_TEST_HWNDS). Refuses if Calculator or Paint is open.
  [ValidateSet('testpad', 'testform', 'testlogin', 'injection', 'testmail', 'notepad', 'calculator', 'realapps')][string]$Fixture = 'testpad',
  # What to press when an approval card shows up: approve, deny, edit (with -EditText), or wait (let it time out).
  [ValidateSet('', 'approve', 'deny', 'edit', 'wait')][string]$Approval = '',
  [string]$EditText = '',
  # Wait this long (ms) before answering an approval, so a recording shows the card.
  [int]$ApprovalDelay = 0,
  [hashtable]$AppEnv = @{},
  [int]$MidShot = 0,
  [ValidateSet('', 'instant', 'fast', 'smooth')][string]$Speed = '',
  [switch]$KeepApp,
  # After the run: the start screen shows no "still running" banner, the reply box's pickers are on, and the
  # task's menu, rename (F2) and delete (with its confirm) work on this run. Deletes the run's folder.
  [switch]$Checks,
  # Record the app window and the fixture window to <Record>-app.mkv and <Record>-pad.mkv (Windows
  # Graphics Capture by window handle: only those two windows end up in the video, nothing behind
  # them). Frames come only when a window changes; tests\demo\make-gif.ps1 fills the gaps.
  [string]$Record = '',
  # Record the screen area of the fixture window, with the agent's overlay (pointer and pill) in it, to
  # <RecordPill>.mkv. The overlay is made visible to capture (REACH_OVERLAY_CAPTURE=1) for this run only.
  [string]$RecordPill = '',
  # A different app home (for example a clean one for a demo recording); config.json is copied in.
  [string]$Home2 = '',
  # the app's Theme setting for this run (system, light or dark); Windows' own setting is untouched
  [ValidateSet('', 'system', 'light', 'dark')][string]$Theme = '',
  [ValidateSet('notepad', 'calculator', 'paint', 'edge', 'todo', 'charmap', 'code')][string[]]$With = @('notepad', 'calculator')
)
$ErrorActionPreference = 'Continue'
$root = (Resolve-Path "$PSScriptRoot\..").Path
$out = "$root\tests\app"
New-Item -ItemType Directory -Force $out | Out-Null
$appHome = "$root\tests\app-home"
if ($Home2) { New-Item -ItemType Directory -Force $Home2 | Out-Null; Copy-Item "$appHome\config.json" "$Home2\config.json" -Force; $appHome = $Home2 }
$drvHome = "$root\tests\drv-home"
# Notepad is one process for all its windows: only use it when it isn't already open, so stopping it never closes the user's own window
if ($Fixture -eq 'notepad' -and (Get-Process -Name Notepad -ErrorAction SilentlyContinue)) { Write-Host 'FAIL Notepad is already open; close it first'; return }
if (($Fixture -eq 'calculator' -or ($Fixture -eq 'realapps' -and 'calculator' -in $With)) -and (Get-Process -Name CalculatorApp -ErrorAction SilentlyContinue)) { Write-Host 'FAIL Calculator is already open; close it first'; return }
if ($Fixture -eq 'realapps' -and 'paint' -in $With -and (Get-Process -Name mspaint -ErrorAction SilentlyContinue)) { Write-Host 'FAIL Paint is already open; close it first'; return }
New-Item -ItemType Directory -Force $appHome, $drvHome | Out-Null
'{"port":7499,"autoPath":false,"agentCursor":false}' | Set-Content -Encoding ascii "$drvHome\config.json"

Add-Type -Name F -Namespace T -MemberDefinition '[DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();' -ErrorAction SilentlyContinue
# Screenshots copy the screen area of a window, so only take one when that window is in front
# (otherwise another of your windows could be in the picture).
function ShotFront($h, $file) {
  Run (J @{ tool = 'apps'; action = 'focus'; window = $h }) | Out-Null; Start-Sleep -Milliseconds 600
  if ([T.F]::GetForegroundWindow().ToInt64() -ne [Convert]::ToInt64($h, 16)) { Log "skipped screenshot ${file}: window not in front"; return }
  $shot = Run (J @{ tool = 'screenshot'; window = $h; force = $true }); if ($shot.path) { Copy-Item $shot.path "$out\$file" -Force }
}
function Run([string]$json) {
  try {
    $r = Invoke-WebRequest -UseBasicParsing -Method Post -Uri 'http://127.0.0.1:7499/run' -Headers @{ Authorization = "Bearer $script:tok" } -ContentType 'application/json' -Body ([Text.Encoding]::UTF8.GetBytes($json))
    return ($r.Content | ConvertFrom-Json)
  } catch {
    $sr = New-Object IO.StreamReader($_.Exception.Response.GetResponseStream()); return ($sr.ReadToEnd() | ConvertFrom-Json)
  }
}
# the harness's own presses (sending the task, Approve) don't wait for the user: only the agent's do
function J($o) { if ($o -is [hashtable] -and $o.tool -and -not $o.ContainsKey('no_yield')) { $o.no_yield = $true }; $o | ConvertTo-Json -Compress -Depth 5 }
function Find($hwnd, $q) { ((Run (J @{ tool = 'ui'; action = 'find'; query = $q; json = $true; limit = 200; window = $hwnd })).text | ConvertFrom-Json).items }
function Hwnd($filter, $proc) {
  $l = (Run (J @{ tool = 'apps'; action = 'list'; filter = $filter; json = $true })).text | ConvertFrom-Json
  ($l | Where-Object { -not $proc -or $_.process -like "$proc*" } | Select-Object -First 1).hwnd
}
function PadText($pad) { if ($Fixture -ne 'testpad') { return '' }; (Run (J @{ tool = 'ui'; action = 'read'; ref = 'Text editor'; window = $pad })).text }
function Log($s) { $script:lines += $s; Write-Host $s }
# Tests run on a second monitor when there is one: windows are moved to the
# non-primary screen's work area. x/y are offsets inside it.
Add-Type -AssemblyName System.Windows.Forms
Add-Type -Name M -Namespace T -MemberDefinition '[DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int cx, int cy, uint f);' -ErrorAction SilentlyContinue
$mon2 = [System.Windows.Forms.Screen]::AllScreens | Where-Object { -not $_.Primary } | Select-Object -First 1
# real apps with only one screen: lay the app and the apps side by side on it, so no window covers another
$single = (-not $mon2) -and $Fixture -eq 'realapps'
if ($single) { $mon2 = [System.Windows.Forms.Screen]::PrimaryScreen }
function PlaceOn2($h, $x, $y, $w, $hh) {
  if (-not $mon2 -or -not $h) { return }
  [void][T.M]::SetWindowPos([IntPtr][Convert]::ToInt64($h, 16), [IntPtr]::Zero, $mon2.WorkingArea.X + $x, $mon2.WorkingArea.Y + $y, $w, $hh, 0x0014)  # NOZORDER|NOACTIVATE
}
function ToMonitor2($h, $x, $y) {
  if (-not $mon2 -or -not $h) { return }
  [void][T.M]::SetWindowPos([IntPtr][Convert]::ToInt64($h, 16), [IntPtr]::Zero, $mon2.WorkingArea.X + $x, $mon2.WorkingArea.Y + $y, 0, 0, 0x0015)  # NOSIZE|NOZORDER|NOACTIVATE
}
$lines = @()

$env:REACH_HOME = $drvHome
$drv = Start-Process "$root\target\release\reach.exe" -ArgumentList 'serve' -WindowStyle Hidden -PassThru
Start-Sleep -Milliseconds 1200
$tok = (Get-Content "$drvHome\token" -Raw).Trim()

Remove-Item Env:REACH_HOME
$titles = @{ calculator = 'Calculator'; notepad = 'Notepad'; testpad = 'Test Pad'; testform = 'Reach Test Form'; testlogin = 'Test Login'; injection = 'Order Status'; testmail = 'Reach Test Mail' }
$sentLog = "$root\tests\out\testmail-sent.log"; Remove-Item $sentLog -ErrorAction SilentlyContinue
$t0pad = Get-Date
if ($Fixture -eq 'notepad') { Start-Process notepad.exe | Out-Null; $pad = $null }
elseif ($Fixture -eq 'calculator') { $pad = $null }
elseif ($Fixture -eq 'realapps') {
  $pad = $null; $real = @(); $edgeProf = "$root\tests\out\edge-profile"
  # where each app goes on the second monitor (x, y, w, h); the first app is the "pad" (step screenshots, recording)
  $slots = @(@(1250, 40, 640, 430), @(1250, 490, 640, 520), @(980, 120, 640, 520))
  if ($single) { $wa = $mon2.WorkingArea; $cw = $wa.Width - 1110; $slots = @(@(1110, 0, $cw, [int]($wa.Height * 0.48)), @(1110, [int]($wa.Height * 0.48), $cw, [int]($wa.Height * 0.52)), @(1110, 0, $cw, $wa.Height)); if ($With.Count -eq 1) { $slots[0] = $slots[2] } }
  foreach ($a in $With) {
    if ($a -eq 'notepad') {
      # classic Notepad under another name: the Store Notepad (which the name notepad.exe redirects to) restores saved tabs
      $np = "$root\tests\out\classic"; New-Item -ItemType Directory -Force "$np\en-US" | Out-Null
      Copy-Item "$env:WINDIR\System32\notepad.exe" "$np\reach-notepad.exe" -Force
      Copy-Item "$env:WINDIR\System32\en-US\notepad.exe.mui" "$np\en-US\reach-notepad.exe.mui" -Force -ErrorAction SilentlyContinue
      $pad = Start-Process "$np\reach-notepad.exe" -PassThru
    }
    elseif ($a -eq 'calculator') { Start-Process 'calculator:' }
    elseif ($a -eq 'paint') { Start-Process 'mspaint' }
    elseif ($a -eq 'charmap') { $cm = Start-Process "$env:WINDIR\System32\charmap.exe" -PassThru }
    elseif ($a -eq 'code') {
      # VS Code with its own temporary profile and a throwaway folder (C:\ReachDemo, one dummy file): none of the
      # user's projects, recent workspaces, settings or accounts
      $codeProf = "$root\tests\out\code-profile"; $proj = 'C:\ReachDemo'
      if (Test-Path $proj) { Remove-Item -Recurse -Force $proj }
      # a fresh profile each run: VS Code restores unsaved files from the last session otherwise
      if (Test-Path $codeProf) { Remove-Item -Recurse -Force $codeProf }
      New-Item -ItemType Directory -Force $codeProf, "$codeProf\User", $proj | Out-Null
      # an existing file, so saving is Ctrl+S and Windows' Save As dialog (Quick access names) never opens
      [IO.File]::WriteAllText("$proj\prices.py", "# Shop prices`n")
      # a plain editor: no AI chat panel, welcome page or extension prompts in the temporary profile
      [IO.File]::WriteAllText("$codeProf\User\settings.json", '{ "chat.disableAIFeatures": true, "workbench.secondarySideBar.defaultVisibility": "hidden", "workbench.startupEditor": "none", "extensions.ignoreRecommendations": true, "telemetry.telemetryLevel": "off", "update.mode": "none", "workbench.tips.enabled": false, "workbench.activityBar.location": "hidden", "git.enabled": false, "security.workspace.trust.enabled": false, "workbench.layoutControl.enabled": false }')
      # open straight on the second monitor: VS Code restores a known folder's window from the profile's saved state
      # (moving it after it appears would flash it on the user's screen first)
      if ($mon2) {
        $sl = $slots[[Array]::IndexOf($With, 'code')]; New-Item -ItemType Directory -Force "$codeProf\User\globalStorage" | Out-Null
        $ui = @{ mode = 1; x = $mon2.WorkingArea.X + $sl[0]; y = $mon2.WorkingArea.Y + $sl[1]; width = $sl[2]; height = $sl[3] }
        $state = @{ windowsState = @{ lastActiveWindow = @{ folder = 'file:///c%3A/ReachDemo'; uiState = $ui }; openedWindows = @(@{ folder = 'file:///c%3A/ReachDemo'; uiState = $ui }) } }
        [IO.File]::WriteAllText("$codeProf\User\globalStorage\storage.json", ($state | ConvertTo-Json -Depth 6))
      }
      $codeExe = @("$env:LOCALAPPDATA\Programs\Microsoft VS Code\Code.exe", "$env:ProgramFiles\Microsoft VS Code\Code.exe") | Where-Object { Test-Path $_ } | Select-Object -First 1
      # prices.py opens with the folder, so the task starts in the editor (opening it from the Explorer cost steps)
      Start-Process $codeExe -ArgumentList "--user-data-dir=`"$codeProf`"", "--extensions-dir=`"$codeProf\ext`"",
        '--new-window', '--disable-workspace-trust', '--skip-welcome', '--skip-release-notes', "`"$proj`"", "`"$proj\prices.py`""
    }
    elseif ($a -in 'edge', 'todo') {
      $sl = $slots[[Array]::IndexOf($With, $a)]
      $page = 'file:///' + ("$root\tests\realapps\$(if ($a -eq 'todo') { 'todo' } else { 'recipe' }).html" -replace '\\', '/')
      Start-Process "${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe" -ArgumentList "--user-data-dir=`"$edgeProf`"", '--inprivate', '--no-first-run', '--no-default-browser-check',
        '--new-window', "--window-position=$($mon2.WorkingArea.X + $sl[0]),$($mon2.WorkingArea.Y + $sl[1])", "--window-size=$($sl[2]),$($sl[3])", $page
    }
  }
  $find = @{ notepad = @('Notepad', 'reach-notepad'); calculator = @('Calculator', 'ApplicationFrameHost'); paint = @('Paint', 'mspaint'); edge = @('Lemon loaf', 'msedge'); todo = @('To-do list', 'msedge')
            charmap = @('Character Map', 'charmap'); code = @('ReachDemo', 'Code') }
  $hw = @{}
  for ($i = 0; $i -lt 60 -and $hw.Count -lt $With.Count; $i++) {
    Start-Sleep -Milliseconds 300
    foreach ($a in $With) { if (-not $hw[$a]) { $h = Hwnd $find[$a][0] $find[$a][1]; if ($h) { $hw[$a] = $h } } }
  }
  for ($i = 0; $i -lt $With.Count; $i++) {
    $a = $With[$i]; $sl = $slots[$i]; if ($a -eq 'calculator') { $sl = @($sl[0], $sl[1], 340, 520) }
    if ($hw[$a]) { PlaceOn2 $hw[$a] $sl[0] $sl[1] $sl[2] $sl[3]; $real += @{ n = $a; h = $hw[$a] } } else { Write-Host "FAIL real app missing: $a" }
  }
  # classic Notepad's "A new version of Notepad is available" banner: press its own Dismiss button
  if ($hw['notepad']) { Run (J @{ tool = 'ui'; action = 'invoke'; ref = 'Dismiss'; window = $hw['notepad'] }) | Out-Null }
  $npH = $hw[$With[0]]; $calcH = $hw['calculator']
  $AppEnv['REACH_TEST_HWNDS'] = ($hw.Values -join ','); if ($pad) { $AppEnv['REACH_TEST_PIDS'] = "$($pad.Id)" }
  $AppEnv['REACH_TEST_SCREEN'] = $find[$With[0]][0]
  $titles['realapps'] = $find[$With[0]][0]
}
else {
  # fixtures that take -X/-Y open straight on the second monitor, so nothing appears on the user's screen first
  $pos = @()
  if ($mon2 -and (Select-String -Path "$root\tests\$Fixture.ps1" -Pattern '\[int\]\$X' -Quiet)) { $pos = '-X', ($mon2.WorkingArea.X + 980), '-Y', ($mon2.WorkingArea.Y + 120) }
  $pad = Start-Process powershell -ArgumentList (@('-STA', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "$root\tests\$Fixture.ps1") + $pos) -WindowStyle Hidden -PassThru
}
# privacy fence: the agent's screenshots and the step thumbnails cover only the (topmost) fixture window
$AppEnv['REACH_TEST_SCREEN'] = $titles[$Fixture]
if ($RecordPill) { $AppEnv['REACH_OVERLAY_CAPTURE'] = '1' }
foreach ($k in $AppEnv.Keys) { Set-Item "Env:$k" $AppEnv[$k] }
if ($Theme) { $cfg = Get-Content "$appHome\config.json" -Raw | ConvertFrom-Json; $cfg.theme = $Theme; $cfg | ConvertTo-Json -Compress | Set-Content -Encoding ascii "$appHome\config.json" }
if ($Speed) { $cfg = Get-Content "$appHome\config.json" -Raw | ConvertFrom-Json; $cfg.motion = $Speed; $cfg | ConvertTo-Json -Compress | Set-Content -Encoding ascii "$appHome\config.json" }
$env:REACH_HOME = $appHome
$env:REACH_TEST_HOTKEY = '1'
if ($mon2) { $env:REACH_WINDOW_POS = "$($mon2.WorkingArea.X + 40),$($mon2.WorkingArea.Y + 40)" }
$app = Start-Process "$root\target\release\reach-app.exe" -PassThru
Remove-Item Env:REACH_HOME, Env:REACH_TEST_HOTKEY, Env:REACH_WINDOW_POS -ErrorAction SilentlyContinue
foreach ($k in $AppEnv.Keys) { Remove-Item "Env:$k" }
try {
  $padH = $null; $appH = $null
  $needPad = $Fixture -ne 'calculator'
  for ($i = 0; $i -lt 60 -and -not (($padH -or -not $needPad) -and $appH); $i++) {
    Start-Sleep -Milliseconds 300
    if ($needPad -and -not $padH) { $padH = if ($Fixture -eq 'realapps') { $npH } else { Hwnd $titles[$Fixture] $(if ($Fixture -eq 'notepad') { 'Notepad' } else { 'powershell' }) } }
    if (-not $appH) { $appH = Hwnd 'Reach' 'reach-app' }
  }
  if (-not (($padH -or -not $needPad) -and $appH)) { Log "FAIL windows missing: pad=$padH app=$appH"; return }
  if ($single) { PlaceOn2 $appH 0 0 1100 $mon2.WorkingArea.Height } else { ToMonitor2 $appH 40 40 }; if ($Fixture -ne 'realapps') { ToMonitor2 $padH 980 120 }; Start-Sleep -Milliseconds 300
  Log "app window $appH, pad window $padH (moved to monitor $(if ($mon2) { $mon2.DeviceName } else { '1: no second monitor' }))"
  # Wait for the brain check (Start enabled only when Claude is ready).
  for ($i = 0; $i -lt 40; $i++) { if (Find $appH 'Describe a task' | Where-Object { $_.enabled }) { break }; Start-Sleep -Milliseconds 300 }
  $box = Find $appH 'Describe a task' | Select-Object -First 1
  $recs = @()
  if ($Record) {
    $wins = @(@{ n = 'app'; h = $appH }, @{ n = 'pad'; h = $padH }); if ($Fixture -eq 'realapps') { $wins = @(@{ n = 'app'; h = $appH }) + @($real | ForEach-Object { @{ n = $_.n; h = $_.h } }) }
    foreach ($w in $wins) {
      $fc = "gfxcapture=hwnd=$([Convert]::ToInt64($w.h, 16)):max_framerate=20:capture_cursor=0,hwdownload,format=bgra,pad=ceil(iw/2)*2:ceil(ih/2)*2,format=yuv444p"
      $psi = New-Object Diagnostics.ProcessStartInfo 'ffmpeg', "-hide_banner -loglevel error -y -filter_complex `"$fc`" -fps_mode vfr -c:v libx264 -crf 12 -preset ultrafast -tune zerolatency -flush_packets 1 `"$Record-$($w.n).mkv`""
      $psi.UseShellExecute = $false; $psi.RedirectStandardInput = $true; $psi.CreateNoWindow = $true
      $psi.EnvironmentVariables['FFREPORT'] = "file=$(("$Record-$($w.n).log") -replace '\\', '/' -replace ':', '\:'):level=24"
      $recs += [Diagnostics.Process]::Start($psi)
    }
    # Windows Graphics Capture sends a frame only when a window changes, and ffmpeg reads 'q' only on a
    # frame: repaint the windows now (first frame) and again before stopping (or a still window never stops)
    Add-Type -Name R -Namespace T -MemberDefinition '[DllImport("user32.dll")] public static extern bool RedrawWindow(IntPtr h, IntPtr r, IntPtr g, uint f);' -ErrorAction SilentlyContinue
    $repaint = { foreach ($w in $wins) { if ($w.h) { [void][T.R]::RedrawWindow([IntPtr][Convert]::ToInt64($w.h, 16), [IntPtr]::Zero, [IntPtr]::Zero, 0x0585) } } }  # INVALIDATE|ERASE|ALLCHILDREN|UPDATENOW|FRAME
    Start-Sleep 2; & $repaint; Log "recording to $Record-app.mkv and -pad.mkv (ffmpeg pids $(($recs | ForEach-Object Id) -join ', '))"
  }
  if ($RecordPill -and $padH) {
    Add-Type -Name P -Namespace T -MemberDefinition '[DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r); public struct RECT { public int L, T, R, B; }' -ErrorAction SilentlyContinue
    $rr = New-Object T.P+RECT; [void][T.P]::GetWindowRect([IntPtr][Convert]::ToInt64($padH, 16), [ref]$rr)
    $w = ($rr.R - $rr.L) - (($rr.R - $rr.L) % 2); $h = ($rr.B - $rr.T) - (($rr.B - $rr.T) % 2)
    $psi = New-Object Diagnostics.ProcessStartInfo 'ffmpeg', "-hide_banner -loglevel error -y -f gdigrab -framerate 20 -draw_mouse 0 -offset_x $($rr.L) -offset_y $($rr.T) -video_size ${w}x$h -i desktop -c:v libx264 -crf 14 -preset ultrafast -pix_fmt yuv444p `"$RecordPill.mkv`""
    $psi.UseShellExecute = $false; $psi.RedirectStandardInput = $true; $psi.CreateNoWindow = $true
    $recs += [Diagnostics.Process]::Start($psi)
    Start-Sleep 1; Log "recording the fixture's screen area ($($rr.L),$($rr.T) ${w}x$h) with the overlay to $RecordPill.mkv"
  }
  if ($Task -eq '') { Start-Sleep 3; ShotFront $appH "$Name-app.jpg"
    Log ((Run (J @{ tool = 'ui'; action = 'inspect'; window = $appH; limit = 12 })).text); return }
  if (-not $box) { Log 'FAIL task box not found'; return }
  $set = Run (J @{ tool = 'ui'; action = 'set'; ref = $box.ref; value = $Task })
  Start-Sleep -Milliseconds 300
  # the redesigned composer's round button is 'Send'; older builds called it 'Start'
  $start = $null
  for ($i = 0; $i -lt 40 -and -not $start; $i++) {
    $start = @(Find $appH 'Send') + @(Find $appH 'Start') | Where-Object { $_.type -eq 'button' -and $_.enabled -and $_.name -in 'Send', 'Start' } | Select-Object -First 1
    if (-not $start) { Start-Sleep -Milliseconds 300 }
  }
  if (-not $start) { Log 'FAIL Send button not enabled'; return }
  $before = @(Get-ChildItem "$appHome\runs" -Directory -ErrorAction SilentlyContinue).Name
  $inv = Run (J @{ tool = 'ui'; action = 'invoke'; ref = $start.ref })
  Log "set: $($set.text) | start: $($inv.text)"
  $t0 = Get-Date
  $runDir = $null
  for ($i = 0; $i -lt 50 -and -not $runDir; $i++) {
    Start-Sleep -Milliseconds 200
    $runDir = Get-ChildItem "$appHome\runs" -Directory -ErrorAction SilentlyContinue | Where-Object { $before -notcontains $_.Name } | Select-Object -First 1
  }
  if (-not $runDir) { Log 'FAIL no run started'; return }
  Log "run $($runDir.Name)"
  $answered = @()
  $state = 'running'; $stopped = $false; $stepAtStop = 0; $mid = $false; $inPad = 0; $samples = 0
  Add-Type -Name C -Namespace T -MemberDefinition '[DllImport("user32.dll")] public static extern bool GetCursorPos(out System.Drawing.Point p); [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r); public struct RECT { public int L, T, R, B; }' -ReferencedAssemblies System.Drawing -ErrorAction SilentlyContinue
  $pr = New-Object T.C+RECT; if ($padH) { [void][T.C]::GetWindowRect([IntPtr][Convert]::ToInt64($padH, 16), [ref]$pr) }
  while (((Get-Date) - $t0).TotalSeconds -lt $TimeoutSec) {
    Start-Sleep -Milliseconds 250
    $ev = @(Get-Content "$($runDir.FullName)\ui.jsonl" -Encoding UTF8 -ErrorAction SilentlyContinue | ForEach-Object { $_ | ConvertFrom-Json })
    $steps = @($ev | Where-Object type -eq 'step').Count
    $last = $ev | Where-Object type -eq 'state' | Select-Object -Last 1
    if ($last) { $state = $last.state }
    # the real pointer: the agent's UI Automation actions must never move it into the window it works in
    $cp = New-Object System.Drawing.Point; [void][T.C]::GetCursorPos([ref]$cp); $samples++
    if ($cp.X -ge $pr.L -and $cp.X -lt $pr.R -and $cp.Y -ge $pr.T -and $cp.Y -lt $pr.B) { $inPad++ }
    if ($MidShot -and -not $mid -and $steps -ge $MidShot) {
      # the fixture is topmost, so its screen area shows only the fixture (and the overlay)
      $mid = $true; $shot = Run (J @{ tool = 'screenshot'; window = $padH; force = $true })
      if ($shot.path) { Copy-Item $shot.path "$out\$Name-mid.jpg" -Force; Log "mid-run shot after $steps steps" }
    }
    if ($TakeOverAfter -gt 0 -and -not $tookOver -and $steps -ge $TakeOverAfter) {
      # Take over: no new steps while held, then Give back and the run goes on
      $tookOver = $true
      $b = Find $appH 'Take over' | Where-Object { $_.type -eq 'button' } | Select-Object -First 1
      $ir = Run (J @{ tool = 'ui'; action = 'invoke'; ref = $b.ref }); $held0 = $steps
      Start-Sleep 4
      $heldSteps = @(Get-Content "$($runDir.FullName)\ui.jsonl" -Encoding UTF8 | Where-Object { $_ -match '"type":"step"' }).Count
      Log "take over: $($ir.text); steps $held0 -> $heldSteps while held 4 s"
      $g = Find $appH 'Give back' | Where-Object { $_.type -eq 'button' } | Select-Object -First 1
      $gr = Run (J @{ tool = 'ui'; action = 'invoke'; ref = $g.ref }); Log "give back: $($gr.text)"
    }
    $card = $ev | Where-Object type -eq 'approval' | Where-Object { $answered -notcontains $_.id } | Select-Object -First 1
    if ($card -and -not ($ev | Where-Object { $_.type -eq 'approval-answer' -and $_.id -eq $card.id })) {
      $answered += $card.id
      Log ("approval card {0} by {1}: {2} | {3} | {4}" -f $card.id, $card.by, $card.action, $card.where, $card.content)
      Start-Sleep -Milliseconds 400; ShotFront $appH "$Name-approval.jpg"
      if ($ApprovalDelay) { Start-Sleep -Milliseconds $ApprovalDelay }
      if ($Approval -eq 'edit') {
        $e1 = Find $appH 'Edit' | Where-Object { $_.type -eq 'button' -and $_.name -eq 'Edit' } | Select-Object -First 1
        Run (J @{ tool = 'ui'; action = 'invoke'; ref = $e1.ref }) | Out-Null; Start-Sleep -Milliseconds 300
        $tb = Find $appH 'Edit the content' | Select-Object -First 1
        Run (J @{ tool = 'ui'; action = 'set'; ref = $tb.ref; value = $EditText }) | Out-Null
        $btn = Find $appH 'Approve with changes' | Where-Object { $_.type -eq 'button' } | Select-Object -First 1
      } elseif ($Approval -in 'approve', 'deny') {
        $label = @{ approve = 'Approve'; deny = 'Deny' }[$Approval]
        $btn = $null  # the log line comes just before the card is drawn: look a few times
        for ($k = 0; $k -lt 20 -and -not $btn; $k++) { $btn = Find $appH $label | Where-Object { $_.type -eq 'button' -and $_.name -eq $label } | Select-Object -First 1; if (-not $btn) { Start-Sleep -Milliseconds 150 } }
      } else { $btn = $null }
      if ($btn) { $ar = Run (J @{ tool = 'ui'; action = 'invoke'; ref = $btn.ref }); Log "pressed $($btn.name): $($ar.text)" }
      elseif ($Approval -in 'approve', 'deny', 'edit') { Log "FAIL approval button not found. ui find: $((Run (J @{ tool = 'ui'; action = 'find'; query = 'Approve'; window = $appH; all = $true })).text)" }
    }
    if ($StopAfter -gt 0 -and -not $stopped -and $steps -ge $StopAfter) {
      $stopped = $true; $stepAtStop = $steps
      if ($StopBy -eq 'button') {
        $raw = Run (J @{ tool = 'ui'; action = 'find'; query = 'Stop'; window = $appH }); Log "find Stop: ok=$($raw.ok) $($raw.text)"
        $b = Find $appH 'Stop' | Where-Object { $_.type -eq 'button' } | Select-Object -First 1
        $ir = Run (J @{ tool = 'ui'; action = 'invoke'; ref = $b.ref }); Log "stop button: $(J $b) -> $($ir.text)"
      } else {
        Add-Type -Name K -Namespace T -MemberDefinition '[DllImport("user32.dll")] public static extern void keybd_event(byte v, byte s, uint f, UIntPtr e);' -ErrorAction SilentlyContinue
        [T.K]::keybd_event(0x11, 0, 0, [UIntPtr]::Zero); [T.K]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero)
        [T.K]::keybd_event(0x1B, 0, 0, [UIntPtr]::Zero); [T.K]::keybd_event(0x1B, 0, 2, [UIntPtr]::Zero)
        [T.K]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero); [T.K]::keybd_event(0x11, 0, 2, [UIntPtr]::Zero)
      }
      Log "pressed Stop ($StopBy) after $steps steps"
    }
    if ($state -in 'waiting', 'done', 'failed', 'stopped') { break }
  }
  Start-Sleep -Milliseconds 800
  $ev = @(Get-Content "$($runDir.FullName)\ui.jsonl" -Encoding UTF8 | ForEach-Object { $_ | ConvertFrom-Json })
  $state = ($ev | Where-Object type -eq 'state' | Select-Object -Last 1).state
  foreach ($e in $ev) {
    switch ($e.type) {
      'step' { Log ("  step {0}: {1} {2} {3} ({4} ms) {5}" -f $e.n, $e.tool, $e.action, (J $e.args), $e.ms, ($e.text -split "`n")[0]) }
      'text' { Log "  agent: $($e.text -replace '\s*\r?\n\s*', ' | ')" }  # one line, so checks can match the whole reply
      'status' { Log "  [$($e.level)] $($e.text)" }
      'approval' { Log "  APPROVAL $($e.id) ($($e.by)): $($e.action) | $($e.where) | $($e.content) | undo $($e.undo)" }
      'approval-answer' { Log "  ANSWER $($e.id): $($e.decision) $($e.text)" }
    }
  }
  $secs = [math]::Round(((Get-Date) - $t0).TotalSeconds, 1)
  Log "real pointer inside the work window: $inPad of $samples samples"
  Log "state: $state, steps: $(@($ev | Where-Object type -eq 'step').Count), wall ${secs}s"
  if ($stopped) {
    $req = $ev | Where-Object type -eq 'stop-request' | Select-Object -First 1
    $fin = $ev | Where-Object { $_.type -eq 'state' -and $_.state -eq 'stopped' } | Select-Object -First 1
    if ($req -and $fin) { Log "stop: requested by $($req.by), stopped state after $($fin.t - $req.t) ms" } else { Log 'FAIL stop not recorded' }
    $claudeLeft = @(Get-CimInstance Win32_Process -Filter "ParentProcessId=$($app.Id)" | Where-Object Name -like 'claude*').Count
    $a = PadText $padH; Start-Sleep 2; $b = PadText $padH
    Log "after stop: claude children left $claudeLeft, pad text stable: $($a -eq $b)"
  }
  Log "pad: $((PadText $padH) -replace "`r?`n", ' / ')"
  if ($Fixture -eq 'calculator') {
    $calcH = Hwnd 'Calculator' 'ApplicationFrameHost'
    Log ("calculator window: " + $(if ($calcH) { "$calcH, " + ((Find $calcH 'Display is' | Select-Object -First 1).name) } else { 'not open' }))
    if ($calcH) { Run (J @{ tool = 'apps'; action = 'close'; window = $calcH }) | Out-Null; Start-Sleep 1 }
  }
  if ($Fixture -eq 'testmail') {
    Log "fake mail status: $((Run (J @{ tool = 'ui'; action = 'read'; ref = 'Status'; window = $padH })).text)"
    Log ("fake mail sent log: " + $(if (Test-Path $sentLog) { (Get-Content $sentLog) -join ' || ' } else { 'none (nothing was sent)' }))
  }
  if ($padH) {
  $shot = Run (J @{ tool = 'screenshot'; window = $padH; force = $true })
    if ($shot.path) { Copy-Item $shot.path "$out\$Name-pad.jpg" -Force }
  }
  ShotFront $appH "$Name-app.jpg"
  if ($Checks) {
    $key = { param($k) Run (J @{ tool = 'apps'; action = 'focus'; window = $appH }) | Out-Null; Run (J @{ tool = 'keyboard'; action = 'press'; text = $k; window = $appH }) | Out-Null; Start-Sleep -Milliseconds 400 }
    $press = { param($q) $c = Find $appH $q | Where-Object { $_.enabled } | Select-Object -First 1; if ($c) { Run (J @{ tool = 'ui'; action = 'invoke'; ref = $c.ref }) | Out-Null; Start-Sleep -Milliseconds 500 }; [bool]$c }
    $pick = Find $appH 'Agent:' | Select-Object -First 1
    Log "CHECK reply box agent picker: $(if ($pick) { "$($pick.name), enabled $($pick.enabled)" } else { 'missing' })"
    # a model switch in the open task adds one line to the thread (a second switch replaces it); back to Claude after
    if ($pick -and $pick.enabled) {
      Run (J @{ tool = 'ui'; action = 'invoke'; ref = $pick.ref }) | Out-Null; Start-Sleep -Milliseconds 400
      $gpt = Find $appH 'ChatGPT' | Where-Object { $_.type -eq 'menuitem' -or $_.type -eq 'radiobutton' } | Select-Object -First 1
      if ($gpt) { Run (J @{ tool = 'ui'; action = 'invoke'; ref = $gpt.ref }) | Out-Null; Start-Sleep -Milliseconds 600 }
      Log "CHECK switch line: $(if (Find $appH 'Switched to ChatGPT for the next message') { 'shown' } else { 'FAIL missing' })"
      $pick = Find $appH 'Agent:' | Select-Object -First 1
      Run (J @{ tool = 'ui'; action = 'invoke'; ref = $pick.ref }) | Out-Null; Start-Sleep -Milliseconds 400
      $cl = Find $appH 'Claude' | Where-Object { $_.type -eq 'menuitem' -or $_.type -eq 'radiobutton' } | Select-Object -First 1
      if ($cl) { Run (J @{ tool = 'ui'; action = 'invoke'; ref = $cl.ref }) | Out-Null; Start-Sleep -Milliseconds 600 }
      Log "CHECK switch back: $(@(Find $appH 'Switched to').Count) switch line(s), $(if (Find $appH 'Switched to Claude') { 'says Claude' } else { 'FAIL' })"
    }
    & $press 'New task' | Out-Null
    $banner = Find $appH 'still running'
    Log "CHECK start screen banner: $(if ($banner) { 'FAIL shown' } else { 'none' })"
    ShotFront $appH "$Name-home.jpg"
    $id = $runDir.Name
    $more = Find $appH 'More for' | Select-Object -First 1; $tname = "$($more.name)" -replace '^More for ', ''
    if (& $press 'More for') { Log "CHECK task menu: $((@('Rename', 'Open run folder', 'Copy task text', 'Stop', 'Delete') | Where-Object { Find $appH $_ | Where-Object { $_.type -eq 'menuitem' } }) -join ', ')"; ShotFront $appH "$Name-menu.jpg"; & $key 'escape' }
    else { Log 'CHECK task menu: FAIL no "More for" button' }
    $row = if ($tname) { Find $appH $tname | Where-Object { $_.type -eq 'button' -and $_.name -like "$tname. *" } | Select-Object -First 1 }
    if ($row) {
      Run (J @{ tool = 'ui'; action = 'focus'; ref = $row.ref }) | Out-Null; & $key 'f2'
      Run (J @{ tool = 'keyboard'; action = 'type'; text = 'Renamed by the test'; window = $appH }) | Out-Null; & $key 'enter'; Start-Sleep -Milliseconds 600
      $t = Get-Content "$appHome\runs\$id\title.txt" -ErrorAction SilentlyContinue
      Log "CHECK rename (F2, Enter): title.txt '$t', row $(if (Find $appH 'Renamed by the test') { 'shows it' } else { 'FAIL old name' })"
      $row = Find $appH 'Renamed by the test' | Where-Object { $_.type -eq 'button' } | Select-Object -First 1
      $fr = Run (J @{ tool = 'ui'; action = 'focus'; ref = $row.ref }); Start-Sleep -Milliseconds 300
      $dr = Run (J @{ tool = 'keyboard'; action = 'press'; text = 'delete'; window = $appH }); Start-Sleep -Milliseconds 500
      Log "  row $($row.name): focus -> $($fr.text); Delete key -> $($dr.text) $($dr.error)"
      $ask = Find $appH "can't be undone"
      if (-not $ask) { # the key didn't reach the row: the menu's Delete opens the same confirm
        if (& $press 'More for') { $mi = Find $appH 'Delete' | Where-Object { $_.type -eq 'menuitem' } | Select-Object -First 1; if ($mi) { Run (J @{ tool = 'ui'; action = 'invoke'; ref = $mi.ref }) | Out-Null; Start-Sleep -Milliseconds 500 } }
        $ask = Find $appH "can't be undone"; Log "  Delete key didn't open the confirm; menu Delete: $(if ($ask) { 'opened it' } else { 'FAIL' })"
      }
      Log "CHECK delete confirm: $(if ($ask) { 'shown' } else { 'FAIL missing' })"
      ShotFront $appH "$Name-confirm.jpg"
      $del = Find $appH 'Delete' | Where-Object { $_.type -eq 'button' -and $_.name -eq 'Delete' } | Select-Object -First 1
      if ($del) { Run (J @{ tool = 'ui'; action = 'invoke'; ref = $del.ref }) | Out-Null; Start-Sleep -Milliseconds 1200 }
      Log "CHECK delete: folder $(if (Test-Path "$appHome\runs\$id") { 'FAIL still there' } else { 'gone' }), row $(if (Find $appH 'Renamed by the test') { 'FAIL still listed' } else { 'gone' })"
    } else { Log 'CHECK rename/delete: FAIL no task row' }
  }
} finally {
  # the pill holds Done for 3 s, so a pill recording runs a little longer
  if ($recs) { Start-Sleep $(if ($RecordPill) { 5 } else { 2 }); if ($repaint) { & $repaint; Start-Sleep -Milliseconds 300 }; foreach ($r in $recs) { if (-not $r.HasExited) { $r.StandardInput.Write('q'); $r.StandardInput.Close() } }
    # ffmpeg finishes on the next frame after q: keep repainting until every recorder has exited (or 10 s)
    for ($i = 0; $i -lt 40 -and ($recs | Where-Object { -not $_.HasExited }); $i++) { if ($repaint) { & $repaint }; Start-Sleep -Milliseconds 250 }
    foreach ($r in $recs) { if (-not $r.HasExited) { Stop-Process -Id $r.Id -Force } } }
  if (-not $KeepApp) { Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue }
  if ($pad) { Stop-Process -Id $pad.Id -Force -ErrorAction SilentlyContinue }
  if ($Fixture -eq 'realapps') {
    Get-Process -Name CalculatorApp, Notepad, mspaint -ErrorAction SilentlyContinue | Where-Object StartTime -ge $t0pad | ForEach-Object { Stop-Process -Id $_.Id -Force }
    # our Edge only: the processes started with our temporary profile (the user's own Edge is left alone)
    if ($cm) { Stop-Process -Id $cm.Id -Force -ErrorAction SilentlyContinue }
    # our VS Code only: the processes started with our temporary profile
    if ($codeProf) { Get-CimInstance Win32_Process -Filter "Name='Code.exe'" | Where-Object { $_.CommandLine -like "*$codeProf*" } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue } }
    Get-CimInstance Win32_Process -Filter "Name='msedge.exe'" | Where-Object { $_.CommandLine -like "*$edgeProf*" } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
  }
  elseif (-not $pad) { Get-Process -Name Notepad, CalculatorApp -ErrorAction SilentlyContinue | Where-Object StartTime -ge $t0pad | ForEach-Object { Stop-Process -Id $_.Id -Force } }
  Stop-Process -Id $drv.Id -Force -ErrorAction SilentlyContinue
  Remove-Item "$drvHome\last.jpg", "$drvHome\zoom.jpg", "$appHome\last.jpg", "$appHome\zoom.jpg" -ErrorAction SilentlyContinue
  $lines | Set-Content -Encoding utf8 "$out\$Name.txt"
}



