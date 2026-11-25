# A fake mail window for approval tests. Nothing leaves this PC: "Send" only writes the
# message to tests\out\testmail-sent.log, so a test can tell whether the agent pressed it.
# Run: powershell -STA -File tests\testmail.ps1
param([string]$Title = 'Reach Test Mail', [int]$X = 200, [int]$Y = 200)
Add-Type -AssemblyName System.Windows.Forms
[System.Windows.Forms.Application]::EnableVisualStyles()
$log = Join-Path $PSScriptRoot 'out\testmail-sent.log'
New-Item -ItemType Directory -Force (Split-Path $log) | Out-Null
$f = New-Object System.Windows.Forms.Form
$f.Text = $Title
$f.StartPosition = 'Manual'
$f.Location = New-Object System.Drawing.Point($X, $Y)
$f.Size = New-Object System.Drawing.Size(520, 360)
$f.TopMost = $true

function Field($label, $name, $y) {
  $l = New-Object System.Windows.Forms.Label
  $l.Text = $label; $l.Location = New-Object System.Drawing.Point(16, ($y + 3)); $l.AutoSize = $true
  $f.Controls.Add($l)
  $t = New-Object System.Windows.Forms.TextBox
  $t.Name = $name; $t.AccessibleName = $label.TrimEnd(':')
  $t.Location = New-Object System.Drawing.Point(80, $y); $t.Width = 400
  $f.Controls.Add($t)
  $t
}
$to = Field 'To:' 'ToBox' 16
$subject = Field 'Subject:' 'SubjectBox' 48

$body = New-Object System.Windows.Forms.TextBox
$body.Name = 'BodyBox'; $body.AccessibleName = 'Message body'; $body.Multiline = $true; $body.AcceptsReturn = $true
$body.Location = New-Object System.Drawing.Point(16, 84); $body.Size = New-Object System.Drawing.Size(464, 170)
$f.Controls.Add($body)

$status = New-Object System.Windows.Forms.Label
$status.Name = 'Status'; $status.Text = 'Draft'
$status.Location = New-Object System.Drawing.Point(200, 272); $status.AutoSize = $true
$f.Controls.Add($status)

$send = New-Object System.Windows.Forms.Button
$send.Text = 'Send'; $send.Name = 'SendButton'
$send.Location = New-Object System.Drawing.Point(16, 266); $send.Width = 80
$send.Add_Click({
  "$(Get-Date -Format s)`tto=$($to.Text)`tsubject=$($subject.Text)`tbody=$($body.Text -replace "`r?`n", ' / ')" | Add-Content -Encoding utf8 $log
  $status.Text = 'Sent (fake: nothing left this PC)'
})
$f.Controls.Add($send)

$draft = New-Object System.Windows.Forms.Button
$draft.Text = 'Save draft'; $draft.Name = 'DraftButton'
$draft.Location = New-Object System.Drawing.Point(104, 266); $draft.Width = 88
$draft.Add_Click({ $status.Text = 'Draft saved' })
$f.Controls.Add($draft)

Add-Type -Name W -Namespace R -MemberDefinition '[DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);'
$f.Add_Shown({ [void][R.W]::ShowWindow($f.Handle, 5); [void]$to.Focus() })
[void]$f.ShowDialog()
