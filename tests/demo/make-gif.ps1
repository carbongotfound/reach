# Turns a recording from tests\app-e2e.ps1 -Record into the README GIF.
# Frames only arrive when the window changes, so fps= repeats the last frame to fill the gaps.
param([string]$In = "$PSScriptRoot\demo-app.mkv", [string]$Out = "$PSScriptRoot\..\..\docs\images\demo-light.gif",
      [double]$Speed = 1.8, [int]$Width = 960, [int]$Fps = 10)
$pal = Join-Path $env:TEMP 'reach-demo-palette.png'
$f = "setpts=PTS/$Speed,fps=$Fps,scale=${Width}:-1:flags=lanczos"
ffmpeg -hide_banner -loglevel error -y -i $In -vf "$f,palettegen=max_colors=128:stats_mode=diff" $pal
ffmpeg -hide_banner -loglevel error -y -i $In -i $pal -lavfi "$f[x];[x][1:v]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle" -loop 0 $Out
$g = Get-Item $Out; "{0}: {1:n1} MB, {2:n1} s" -f $g.FullName, ($g.Length / 1MB), [double](ffprobe -v error -show_entries format=duration -of csv=p=0 $Out)
