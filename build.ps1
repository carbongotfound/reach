# Builds reach.exe with the C# compiler that ships with .NET Framework 4.x.
# No SDK, no NuGet. Usage:  powershell -ExecutionPolicy Bypass -File build.ps1
param([string]$Out = "reach.exe")
$ErrorActionPreference = "Stop"
$fw = Join-Path $env:WINDIR "Microsoft.NET\Framework64\v4.0.30319"
$csc = Join-Path $fw "csc.exe"
if (-not (Test-Path $csc)) { throw "csc.exe not found at $csc (needs .NET Framework 4.8)" }
$wpf = Join-Path $fw "WPF"
$src = Join-Path $PSScriptRoot "src\Reach.cs"
$outPath = if ([IO.Path]::IsPathRooted($Out)) { $Out } else { Join-Path $PSScriptRoot $Out }
& $csc /nologo /target:exe /platform:x64 /optimize+ /debug- /nowarn:0649 `
    /r:System.Windows.Forms.dll /r:System.Drawing.dll /r:System.Web.Extensions.dll `
    "/r:$wpf\UIAutomationClient.dll" "/r:$wpf\UIAutomationTypes.dll" "/r:$wpf\WindowsBase.dll" `
    "/out:$outPath" $src
if ($LASTEXITCODE -ne 0) { throw "build failed" }
"built $outPath ($((Get-Item $outPath).Length) bytes)"
