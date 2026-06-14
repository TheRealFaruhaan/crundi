# Registers PM2 resurrect as a Windows login startup task via Task Scheduler.
# Run once after "pm2 save". No admin required.

$taskName = "PM2-crundi"
$pm2Path  = (Get-Command pm2 -ErrorAction SilentlyContinue).Source
$nodePath = (Get-Command node -ErrorAction SilentlyContinue).Source

if (-not $pm2Path) {
    Write-Error "pm2 not found in PATH. Install it with: npm install -g pm2"
    exit 1
}
if (-not $nodePath) {
    Write-Error "node not found in PATH."
    exit 1
}

# Task Scheduler needs the actual .cmd wrapper, not the shim
$pm2Cmd = Join-Path (Split-Path $pm2Path) "pm2.cmd"
if (-not (Test-Path $pm2Cmd)) { $pm2Cmd = $pm2Path }

# Remove existing task if present
Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue

$action  = New-ScheduledTaskAction -Execute $pm2Cmd -Argument "resurrect"
$trigger = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME
$settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit (New-TimeSpan -Seconds 0) -StartWhenAvailable

Register-ScheduledTask `
    -TaskName $taskName `
    -Action $action `
    -Trigger $trigger `
    -Settings $settings `
    -RunLevel Limited `
    -Force | Out-Null

Write-Host "Startup task '$taskName' registered." -ForegroundColor Green
Write-Host "PM2 will resurrect saved processes on next login."
Write-Host ""
Write-Host "To remove:  schtasks /Delete /TN $taskName /F"
