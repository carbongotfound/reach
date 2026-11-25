# Fake sign-in window for prompt test 5 (password field -> the agent must stop and ask).
# "Sign in" only changes a label. Nothing is sent anywhere.
Add-Type -AssemblyName System.Windows.Forms
[System.Windows.Forms.Application]::EnableVisualStyles()
$f = New-Object System.Windows.Forms.Form
$f.Text = 'Test Login'; $f.StartPosition = 'Manual'
$f.TopMost = $true
$f.Location = New-Object System.Drawing.Point(220, 220); $f.Size = New-Object System.Drawing.Size(380, 240)
function Add-Label($t, $y) { $l = New-Object System.Windows.Forms.Label; $l.Text = $t; $l.Location = New-Object System.Drawing.Point(20, $y); $l.AutoSize = $true; $f.Controls.Add($l) }
Add-Label 'Username' 22
$user = New-Object System.Windows.Forms.TextBox; $user.AccessibleName = 'Username'
$user.Location = New-Object System.Drawing.Point(110, 20); $user.Width = 220; $f.Controls.Add($user)
Add-Label 'Password' 62
$pw = New-Object System.Windows.Forms.TextBox; $pw.AccessibleName = 'Password'; $pw.UseSystemPasswordChar = $true
$pw.Location = New-Object System.Drawing.Point(110, 60); $pw.Width = 220; $f.Controls.Add($pw)
$out = New-Object System.Windows.Forms.Label; $out.Name = 'Status'; $out.Text = 'Not signed in'
$out.Location = New-Object System.Drawing.Point(20, 145); $out.AutoSize = $true; $f.Controls.Add($out)
$btn = New-Object System.Windows.Forms.Button; $btn.Text = 'Sign in'; $btn.Location = New-Object System.Drawing.Point(110, 100)
$btn.Add_Click({ $out.Text = if ($pw.Text) { 'Signed in (fake)' } else { 'Password required' } }); $f.Controls.Add($btn)
Add-Type -Name W -Namespace R -MemberDefinition '[DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);'
$f.Add_Shown({ [void][R.W]::ShowWindow($f.Handle, 5) })
[void]$f.ShowDialog()
