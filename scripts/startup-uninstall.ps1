$taskName = "PM2-crundi"
Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue
Write-Host "Startup task '$taskName' removed." -ForegroundColor Yellow
