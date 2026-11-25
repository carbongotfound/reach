# A plain text editor test window ("Test Pad") for app end-to-end tests, used instead
# of Windows Notepad, which restores the user's previous tabs (private content).
# Ctrl+S / File > Save opens a small Save As box of its own; the file goes in -SaveDir.
param([int]$X = 160, [int]$Y = 160, [string]$SaveDir = "$PSScriptRoot\..\tests\out")
Add-Type -AssemblyName System.Windows.Forms
[System.Windows.Forms.Application]::EnableVisualStyles()
New-Item -ItemType Directory -Force $SaveDir | Out-Null
$f = New-Object System.Windows.Forms.Form
$f.Text = 'Untitled - Test Pad'
$f.TopMost = $true
$f.StartPosition = 'Manual'
$f.Location = New-Object System.Drawing.Point($X, $Y)
$f.Size = New-Object System.Drawing.Size(640, 420)

$box = New-Object System.Windows.Forms.TextBox
$box.Multiline = $true; $box.AcceptsReturn = $true; $box.AcceptsTab = $true
$box.ScrollBars = 'Vertical'; $box.Dock = 'Fill'
$box.Font = New-Object System.Drawing.Font('Consolas', 11)
$box.Name = 'Editor'; $box.AccessibleName = 'Text editor'

$menu = New-Object System.Windows.Forms.MenuStrip
$file = New-Object System.Windows.Forms.ToolStripMenuItem('&File')
$save = New-Object System.Windows.Forms.ToolStripMenuItem('&Save')
$save.ShortcutKeys = [System.Windows.Forms.Keys]::Control -bor [System.Windows.Forms.Keys]::S
$script:path = $null
$save.Add_Click({
  if (-not $script:path) {
    # its own small Save As box, not Windows' file dialog: that one shows Quick access, drive names
    # and the folder path, which would end up in test screenshots and recordings
    $d = New-Object System.Windows.Forms.Form
    $d.Text = 'Save As'; $d.FormBorderStyle = 'FixedDialog'; $d.MinimizeBox = $false; $d.MaximizeBox = $false
    $d.StartPosition = 'CenterParent'; $d.ClientSize = New-Object System.Drawing.Size(360, 110); $d.TopMost = $true
    $l = New-Object System.Windows.Forms.Label; $l.Text = 'File &name:'; $l.AutoSize = $true; $l.Location = New-Object System.Drawing.Point(14, 18)
    $n = New-Object System.Windows.Forms.TextBox; $n.Text = '*.txt'; $n.AccessibleName = 'File name'; $n.Location = New-Object System.Drawing.Point(90, 15); $n.Width = 254
    $ok = New-Object System.Windows.Forms.Button; $ok.Text = '&Save'; $ok.DialogResult = 'OK'; $ok.Location = New-Object System.Drawing.Point(184, 66)
    $no = New-Object System.Windows.Forms.Button; $no.Text = 'Cancel'; $no.DialogResult = 'Cancel'; $no.Location = New-Object System.Drawing.Point(268, 66)
    $d.Controls.AddRange(@($l, $n, $ok, $no)); $d.AcceptButton = $ok; $d.CancelButton = $no
    $d.Add_Shown({ $n.SelectAll(); [void]$n.Focus() })
    if ($d.ShowDialog($f) -ne 'OK') { return }
    $name = [IO.Path]::GetFileName($n.Text.Trim())
    if (-not $name -or $name.Contains('*')) { return }
    if (-not [IO.Path]::HasExtension($name)) { $name += '.txt' }
    $script:path = Join-Path $SaveDir $name
  }
  [IO.File]::WriteAllText($script:path, $box.Text)
  $f.Text = [IO.Path]::GetFileName($script:path) + ' - Test Pad'
})
[void]$file.DropDownItems.Add($save)
[void]$menu.Items.Add($file)
$f.Controls.Add($box)
$f.Controls.Add($menu)
$f.MainMenuStrip = $menu

Add-Type -Name W -Namespace R -MemberDefinition '[DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);'
$f.Add_Shown({ [void][R.W]::ShowWindow($f.Handle, 5); [void]$box.Focus() })
[void]$f.ShowDialog()
