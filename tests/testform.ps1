# A harmless test window for Reach: a text box, a button, a checkbox, a list.
# Nothing here sends, saves or submits anything. Run: powershell -STA -File tests\testform.ps1
param([string]$Title = 'Reach Test Form', [int]$X = 200, [int]$Y = 200)
Add-Type -AssemblyName System.Windows.Forms
[System.Windows.Forms.Application]::EnableVisualStyles()
$f = New-Object System.Windows.Forms.Form
$f.Text = $Title
$f.StartPosition = 'Manual'
$f.Location = New-Object System.Drawing.Point($X, $Y)
$f.Size = New-Object System.Drawing.Size(460, 320)
$f.TopMost = $true

$name = New-Object System.Windows.Forms.TextBox
$name.Name = 'NameBox'; $name.AccessibleName = 'Name'
$name.Location = New-Object System.Drawing.Point(20, 20); $name.Width = 300
$f.Controls.Add($name)

$out = New-Object System.Windows.Forms.Label
$out.Name = 'Output'; $out.Text = 'Waiting'  # no AccessibleName: UIA Name is the live text, aid is 'Output'
$out.Location = New-Object System.Drawing.Point(20, 60); $out.AutoSize = $true
$f.Controls.Add($out)

$btn = New-Object System.Windows.Forms.Button
$btn.Text = 'Say Hi'; $btn.Name = 'SayHi'
$btn.Location = New-Object System.Drawing.Point(20, 95); $btn.Width = 120
$btn.Add_Click({ $out.Text = 'Hi ' + $name.Text })
$f.Controls.Add($btn)

$chk = New-Object System.Windows.Forms.CheckBox
$chk.Text = 'Agree'; $chk.Name = 'Agree'
$chk.Location = New-Object System.Drawing.Point(20, 135)
$f.Controls.Add($chk)

$list = New-Object System.Windows.Forms.ListBox
$list.Name = 'Fruit'; $list.AccessibleName = 'Fruit'
$list.Location = New-Object System.Drawing.Point(180, 135); $list.Size = New-Object System.Drawing.Size(140, 80)
[void]$list.Items.AddRange(@('Apple', 'Banana', 'Cherry'))
$f.Controls.Add($list)

$clicks = 0
$area = New-Object System.Windows.Forms.Panel
$area.Name = 'ClickArea'; $area.AccessibleName = 'Click area'; $area.BackColor = 'LightSteelBlue'
$area.Location = New-Object System.Drawing.Point(20, 230); $area.Size = New-Object System.Drawing.Size(300, 30)
$area.Add_MouseClick({ $script:clicks++; $out.Text = "Clicked $script:clicks" })
$f.Controls.Add($area)

# Launched with a hidden console: the first ShowWindow inherits SW_HIDE, so show it again.
Add-Type -Name W -Namespace R -MemberDefinition '[DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);'
$f.Add_Shown({ [void][R.W]::ShowWindow($f.Handle, 5); [void]$btn.Focus() })  # no blinking caret: stable screenshots
[void]$f.ShowDialog()


