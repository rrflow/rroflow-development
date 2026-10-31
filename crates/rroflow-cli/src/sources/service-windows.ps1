param([Parameter(Mandatory=$true)][ValidateSet('install','status','start','stop','remove')][string]$Action,
      [Parameter(Mandatory=$true)][string]$Manifest)
$ErrorActionPreference = 'Stop'
$install = Get-Content -LiteralPath $Manifest -Raw | ConvertFrom-Json
$identity = [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$name = 'rroflow-source-' + $install.source_id
$description = 'rroflow.startup/v1 ' + $install.source_id + ' ' + $install.sha256
$worker = Join-Path (Split-Path -Parent $Manifest) 'worker.ps1'
# Windows paths cannot contain quotes. Values remain arguments, never script text.
if ($Manifest.Contains('"') -or $worker.Contains('"')) { throw 'Invalid startup path.' }
$powershell = Join-Path $PSHOME 'powershell.exe'
$arguments = '-NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File "' + $worker + '" -Manifest "' + $Manifest + '"'
$task = Get-ScheduledTask -TaskName $name -TaskPath '\' -ErrorAction SilentlyContinue
function Sid([string]$value) {
    if ($value.StartsWith('S-1-')) { return $value }
    return ([System.Security.Principal.NTAccount]::new($value)).Translate([System.Security.Principal.SecurityIdentifier]).Value
}
if ($task) {
    if ($task.Description -ne $description -or $task.Actions.Count -ne 1 -or
        $task.Actions[0].Execute -ne $powershell -or $task.Actions[0].Arguments -ne $arguments -or
        (Sid $task.Principal.UserId) -ne $identity -or $task.Principal.LogonType -ne 'Interactive' -or
        $task.Principal.RunLevel -ne 'Limited') { throw 'Another task owns this name or the definition changed. Nothing was changed.' }
    if ($task.Triggers.Count -ne 1 -or $task.Triggers[0].CimClass.CimClassName -ne 'MSFT_TaskLogonTrigger' -or (Sid $task.Triggers[0].UserId) -ne $identity) { throw 'Startup trigger changed. Nothing was changed.' }
}
if ($Action -eq 'install' -and !$task) {
    $principal = New-ScheduledTaskPrincipal -UserId $identity -LogonType Interactive -RunLevel Limited
    $trigger = New-ScheduledTaskTrigger -AtLogOn -User $identity
    $settings = New-ScheduledTaskSettingsSet -StartWhenAvailable -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -MultipleInstances IgnoreNew -ExecutionTimeLimit ([TimeSpan]::Zero) -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1)
    $exec = New-ScheduledTaskAction -Execute $powershell -Argument $arguments
    Register-ScheduledTask -TaskName $name -TaskPath '\' -Action $exec -Principal $principal -Trigger $trigger -Settings $settings -Description $description | Out-Null
    $task = Get-ScheduledTask -TaskName $name -TaskPath '\'
}
switch ($Action) {
    'install' { Start-ScheduledTask -TaskName $name -TaskPath '\' }
    'start' { if (!$task) { throw 'Startup task is missing; reinstall it.' }; Start-ScheduledTask -TaskName $name -TaskPath '\' }
    'stop' { if ($task) { Stop-ScheduledTask -TaskName $name -TaskPath '\' } }
    'remove' { if ($task) { Stop-ScheduledTask -TaskName $name -TaskPath '\'; Unregister-ScheduledTask -TaskName $name -TaskPath '\' -Confirm:$false } }
}
$task = Get-ScheduledTask -TaskName $name -TaskPath '\' -ErrorAction SilentlyContinue
$info = if ($task) { Get-ScheduledTaskInfo -TaskName $name -TaskPath '\' }
@{manager='windows-task-scheduler'; task=$name; state=$(if ($task) { [string]$task.State } else { 'Not registered' }); last_exit_code=$(if ($info) { $info.LastTaskResult } else { $null }); note='OS state only; inspect source reports for delivery. Runs while this Windows user is logged in; closing a terminal or disconnecting RDP is different from logging out.'} | ConvertTo-Json -Compress
